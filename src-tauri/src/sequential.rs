//! One batch, one live profile, using the launcher's existing lifecycle and a restricted page check.
use anyhow::{bail, Context, Result};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;

const START_TIMEOUT: Duration = Duration::from_secs(90);
const NAV_TIMEOUT: Duration = Duration::from_secs(60);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(100);
const MAX_BATCH: u32 = 1000;
fn default_wait() -> u64 {
    15
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub start: u32,
    pub end: u32,
    pub url: String,
    #[serde(default = "default_wait")]
    pub wait_seconds: u64,
}
impl Config {
    fn validate(&self) -> Result<()> {
        if self.start > self.end || self.end - self.start >= MAX_BATCH {
            bail!("select an ascending range of at most {MAX_BATCH} profiles");
        }
        if !(1..=86400).contains(&self.wait_seconds) {
            bail!("wait must be between 1 and 86400 seconds");
        }
        let u = url::Url::parse(&self.url).context("invalid website URL")?;
        if !matches!(u.scheme(), "http" | "https")
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
        {
            bail!("use an absolute HTTP or HTTPS URL without embedded credentials");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub name: String,
    pub id: Option<String>,
    pub error: Option<String>,
}
fn resolve(cfg: &Config, profiles: &[(String, String)]) -> Result<Vec<Item>> {
    cfg.validate()?;
    Ok((cfg.start..=cfg.end)
        .map(|n| {
            let matches: Vec<_> = profiles
                .iter()
                .filter(|(_, name)| {
                    !name.is_empty()
                        && name.bytes().all(|c| c.is_ascii_digit())
                        && name.parse::<u32>().ok() == Some(n)
                })
                .collect();
            match matches.as_slice() {
                [p] => Item {
                    name: p.1.clone(),
                    id: Some(p.0.clone()),
                    error: None,
                },
                [] => Item {
                    name: format!("{n:03}"),
                    id: None,
                    error: Some("profile not found".into()),
                },
                _ => Item {
                    name: format!("{n:03}"),
                    id: None,
                    error: Some("ambiguous numeric name: rename duplicate profiles".into()),
                },
            }
        })
        .collect())
}
#[derive(Clone, Debug, Serialize)]
pub struct Record {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct State {
    pub running: bool,
    pub phase: String,
    pub current: Option<String>,
    pub current_id: Option<String>,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<Record>,
    pub error: Option<String>,
}
struct Run {
    state: Mutex<State>,
    stop: watch::Sender<bool>,
    native: Native,
}
impl Run {
    fn new(total: usize) -> Self {
        Self {
            state: Mutex::new(State {
                running: true,
                phase: "starting".into(),
                total,
                ..State::default()
            }),
            stop: watch::channel(false).0,
            native: Native::default(),
        }
    }
    fn change(&self, f: impl FnOnce(&mut State)) {
        let snapshot = {
            let mut s = self.state.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        if let Some(app) = crate::app_handle() {
            use tauri::Emitter;
            let _ = app.emit("sequential-progress", snapshot);
        }
    }
    fn phase(&self, name: &str, phase: &str) {
        eprintln!("[Automation] {name}: {phase}");
        self.change(|s| {
            s.current = Some(name.into());
            s.phase = phase.into();
        });
    }
}
fn slot() -> &'static Mutex<Option<Arc<Run>>> {
    static S: OnceLock<Mutex<Option<Arc<Run>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}
pub fn status() -> State {
    slot()
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.state.lock().unwrap().clone())
        .unwrap_or_default()
}
pub fn active() -> bool {
    let s = status();
    s.running || s.current_id.is_some()
}
tokio::task_local! { static OWNER: (); }
pub fn launch_allowed() -> bool {
    !active() || OWNER.try_with(|_| ()).is_ok()
}
// Shared with every launch: the batch admission check cannot race a manual/API launch.
pub fn launch_gate() -> &'static tokio::sync::Mutex<()> {
    static G: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    G.get_or_init(|| tokio::sync::Mutex::new(()))
}

trait Lifecycle: Send + Sync {
    fn start<'a>(&'a self, id: &'a str, url: &'a str) -> BoxFuture<'a, Result<()>>;
    fn navigate<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>>;
    fn stop<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>>;
    fn running(&self, id: &str) -> bool;
}
#[derive(Default)]
struct Native {
    #[cfg(feature = "automation")]
    page: tokio::sync::Mutex<Option<PageReport>>,
}

#[cfg(feature = "automation")]
struct PageReport {
    receiver: tokio::sync::oneshot::Receiver<Result<()>>,
    server: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "automation")]
impl Drop for PageReport {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[cfg(feature = "automation")]
impl PageReport {
    async fn listen(url: &str) -> Result<(Self, crate::startup_page::Probe)> {
        use axum::{
            extract::Query,
            http::{header, HeaderMap, StatusCode},
            routing::post,
            Router,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let probe = crate::startup_page::Probe {
            addr: listener.local_addr()?,
            nonce: uuid::Uuid::new_v4(),
            target: url::Url::parse(url)?,
        };
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let origin = format!("chrome-extension://{}", crate::startup_page::ID);
        let app = Router::new().route(
            &format!("/{}", probe.nonce),
            post(move |headers: HeaderMap, Query(query): Query<std::collections::HashMap<String, String>>| async move {
                if !headers
                    .get(header::ORIGIN)
                    .is_some_and(|value| value == origin.as_str())
                {
                    return StatusCode::FORBIDDEN;
                }
                let outcome = match query.get("outcome").map(String::as_str) {
                    Some("ok") => Ok(()),
                    Some("unexpected_page_origin") => Err(anyhow::anyhow!("browser opened a different page origin")),
                    Some("unreadable_page") => Err(anyhow::anyhow!("browser page did not provide readable content")),
                    Some("page_check_failed") => Err(anyhow::anyhow!("browser could not check the page")),
                    _ => return StatusCode::BAD_REQUEST,
                };
                match sender.lock().unwrap().take() {
                    Some(sender) if !sender.is_closed() => {
                        let _ = sender.send(outcome);
                        StatusCode::NO_CONTENT
                    }
                    _ => StatusCode::GONE,
                }
            }),
        );
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok((Self { receiver, server }, probe))
    }
}

impl Lifecycle for Native {
    fn start<'a>(&'a self, id: &'a str, url: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            #[cfg(feature = "automation")]
            {
                let (report, probe) = PageReport::listen(url).await?;
                *self.page.lock().await = Some(report);
                crate::launch::launch_sequential(id, &probe).await?;
                Ok(())
            }
            #[cfg(not(feature = "automation"))]
            {
                let _ = (id, url);
                bail!("automation is not enabled in this build")
            }
        })
    }
    fn navigate<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            #[cfg(feature = "automation")]
            {
                let mut page = self.page.lock().await;
                let report = page.as_mut().context("page check was not started")?;
                loop {
                    if !self.running(id) {
                        bail!("browser closed during navigation");
                    }
                    tokio::select! {
                        result = &mut report.receiver => return result.context("page check ended without readable content")?,
                        _ = tokio::time::sleep(POLL) => {}
                    }
                }
            }
            #[cfg(not(feature = "automation"))]
            {
                let _ = id;
                bail!("automation is not enabled in this build")
            }
        })
    }
    fn stop<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            close_profile(id).await?;
            #[cfg(feature = "automation")]
            self.page.lock().await.take();
            Ok(())
        })
    }
    fn running(&self, id: &str) -> bool {
        crate::process::Tracker::shared().is_running(id)
    }
}

async fn close_profile(id: &str) -> Result<()> {
    let tracker = crate::process::Tracker::shared();
    tracker.request_graceful_close(id).await;
    // Tracker releases the tunnel before dropping the entry, and stays non-idle
    // until the profile exit hooks finish saving its data and releasing leases.
    while !tracker.is_idle() {
        tokio::time::sleep(POLL).await;
    }
    Ok(())
}
async fn cancel(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
async fn step<T>(
    rx: &mut watch::Receiver<bool>,
    limit: Duration,
    op: &str,
    f: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! { biased;
        _ = cancel(rx) => bail!("cancelled"),
        result = tokio::time::timeout(limit, f) => result.with_context(|| format!("{op} timed out"))?,
    }
}
async fn drive<L: Lifecycle>(run: Arc<Run>, cfg: Config, items: Vec<Item>, life: &L) {
    let mut rx = run.stop.subscribe();
    let mut fatal = None;
    for item in items {
        if *rx.borrow() {
            break;
        }
        let Some(id) = item.id.as_deref() else {
            run.change(|s| {
                s.failed += 1;
                s.results.push(Record {
                    name: item.name,
                    status: "failed".into(),
                    error: item.error,
                });
            });
            continue;
        };
        run.change(|s| s.current_id = Some(id.into()));
        let result: Result<()> = async {
            run.phase(&item.name, "starting");
            step(
                &mut rx,
                START_TIMEOUT,
                "startup/readiness",
                life.start(id, &cfg.url),
            )
            .await?;
            run.phase(&item.name, "navigating");
            step(&mut rx, NAV_TIMEOUT, "navigation", life.navigate(id)).await?;
            run.phase(&item.name, &format!("waiting {} seconds", cfg.wait_seconds));
            step(
                &mut rx,
                Duration::from_secs(cfg.wait_seconds + 2),
                "wait",
                async {
                    let until = tokio::time::Instant::now() + Duration::from_secs(cfg.wait_seconds);
                    while tokio::time::Instant::now() < until {
                        if !life.running(id) {
                            bail!("browser closed before wait finished");
                        }
                        tokio::time::sleep(
                            POLL.min(until.saturating_duration_since(tokio::time::Instant::now())),
                        )
                        .await;
                    }
                    Ok(())
                },
            )
            .await
        }
        .await;
        run.phase(&item.name, "stopping");
        // Cleanup is not cancelled: Stop must wait for the owned child to exit.
        let cleanup = tokio::time::timeout(STOP_TIMEOUT, life.stop(id)).await;
        let error = match cleanup {
            Ok(Ok(())) if !life.running(id) => result.err().map(|e| format!("{e:#}")),
            other => {
                let e = format!("shutdown could not be confirmed ({other:?}); batch halted, close this profile before retrying");
                fatal = Some(e.clone());
                Some(e)
            }
        };
        if fatal.is_none() {
            run.change(|s| s.current_id = None);
        }
        let stopped = *rx.borrow();
        if let Some(e) = &error {
            eprintln!("[Automation] {}: {e}", item.name);
        }
        run.change(|s| {
            let status = if stopped {
                "cancelled"
            } else if error.is_some() {
                s.failed += 1;
                "failed"
            } else {
                s.completed += 1;
                "completed"
            };
            s.results.push(Record {
                name: item.name,
                status: status.into(),
                error,
            });
        });
        if fatal.is_some() || stopped {
            break;
        }
    }
    run.change(|s| {
        s.running = false;
        s.phase = if fatal.is_some() {
            "cleanup failed"
        } else if *rx.borrow() {
            "stopped"
        } else {
            "complete"
        }
        .into();
        s.error = fatal;
    });
}
#[tauri::command]
pub fn sequential_status() -> State {
    status()
}
#[tauri::command]
pub async fn sequential_start(config: Config) -> Result<(), String> {
    start(config).await.map_err(|e| format!("{e:#}"))
}
async fn start(config: Config) -> Result<()> {
    if !cfg!(feature = "automation") {
        bail!("automation is not enabled in this build");
    }
    let _gate = launch_gate().lock().await;
    if crate::safe_close::active() {
        bail!("ShyShard is closing.");
    }
    if active() {
        bail!("a sequential batch is already running");
    }
    if crate::migrate::in_progress() {
        bail!("wait for data migration to finish");
    }
    if crate::launch::in_flight() != 0 || !crate::process::Tracker::shared().running().is_empty() {
        bail!("close all profiles before starting a sequential batch");
    }
    #[cfg(feature = "automation")]
    if crate::runner::all().iter().any(|r| r.running) {
        bail!("stop other automation projects first");
    }
    // User switches must not bypass the startup helper or replace its profile.
    for a in crate::settings::parse_extra_args(&crate::settings::load()?.extra_args) {
        if matches!(
            a.split('=').next(),
            Some(
                "--user-data-dir"
                    | "--profile-directory"
                    | "--incognito"
                    | "--guest"
                    | "--remote-debugging-port"
                    | "--remote-debugging-pipe"
                    | "--disable-extensions"
                    | "--disable-extensions-except"
                    | "--load-extension"
                    | "--headless"
                    | "--no-startup-window"
            )
        ) {
            bail!("remove conflicting extra launch argument: {a}");
        }
    }
    let profiles = crate::space::profile::list_all()?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect::<Vec<_>>();
    let items = resolve(&config, &profiles)?;
    let run = Arc::new(Run::new(items.len()));
    *slot().lock().unwrap() = Some(run.clone());
    tokio::spawn(OWNER.scope((), async move {
        drive(run.clone(), config, items, &run.native).await;
    }));
    Ok(())
}
pub async fn stop_and_wait() -> Result<()> {
    let Some(run) = slot().lock().unwrap().clone() else {
        return Ok(());
    };
    run.stop.send_replace(true);
    tokio::time::timeout(STOP_TIMEOUT + Duration::from_secs(5), async {
        while run.state.lock().unwrap().running {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .context("still stopping the batch")?;
    let snapshot = run.state.lock().unwrap().clone();
    if let Some(id) = snapshot.current_id.as_deref() {
        let _gate = launch_gate().lock().await;
        tokio::time::timeout(STOP_TIMEOUT, run.native.stop(id))
            .await
            .context("shutdown retry timed out")??;
        if crate::process::Tracker::shared().is_running(id) {
            bail!("profile {id} is still running");
        }
        run.change(|s| {
            s.error = None;
            s.current_id = None;
            s.phase = "stopped".into();
        });
    }
    Ok(())
}
#[tauri::command]
pub async fn sequential_stop() -> Result<(), String> {
    stop_and_wait().await.map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            start: 1,
            end: 50,
            url: "https://example.com".into(),
            wait_seconds: 15,
        }
    }
    fn profiles() -> Vec<(String, String)> {
        (1..=50)
            .rev()
            .map(|i| (format!("uuid-{i}"), format!("{i:03}")))
            .collect()
    }
    #[test]
    fn numeric_range_resolves_ids_not_names() {
        let plan = resolve(&config(), &profiles()).unwrap();
        assert_eq!(plan.len(), 50);
        for (i, p) in plan.iter().enumerate() {
            assert_eq!(p.id.as_deref(), Some(format!("uuid-{}", i + 1).as_str()));
        }
        for i in [1, 2, 9, 10, 50] {
            assert_eq!(plan[i - 1].name, format!("{i:03}"));
        }
    }
    #[test]
    fn missing_and_ambiguous_names_are_explicit() {
        let p = resolve(
            &config(),
            &[
                ("a".into(), "001".into()),
                ("b".into(), "1".into()),
                ("c".into(), "010".into()),
            ],
        )
        .unwrap();
        assert!(p[0].error.as_ref().unwrap().contains("ambiguous"));
        assert!(p[1].error.as_ref().unwrap().contains("not found"));
        assert_eq!(p[9].id.as_deref(), Some("c"));
    }
    #[test]
    fn rejects_bad_ranges_urls_and_waits() {
        let mut c = config();
        for url in [
            "example.com",
            "file:///tmp/a",
            "javascript:alert(1)",
            "https://user:secret@example.com",
        ] {
            c.url = url.into();
            assert!(c.validate().is_err());
        }
        c = config();
        c.start = 51;
        assert!(c.validate().is_err());
        c = config();
        c.end = 1001;
        assert!(c.validate().is_err());
        c = config();
        for wait in [0, 86401, u64::MAX] {
            c.wait_seconds = wait;
            assert!(c.validate().is_err());
        }
        let c: Config =
            serde_json::from_str(r#"{"start":1,"end":50,"url":"https://example.com"}"#).unwrap();
        assert_eq!(c.wait_seconds, 15);
        assert!(c.validate().is_ok());
    }

    struct Fake {
        run: Arc<Run>,
        events: Mutex<Vec<String>>,
        live: Mutex<Option<String>>,
        fail: Option<&'static str>,
        cancel_at: Option<&'static str>,
    }

    impl Fake {
        fn new(run: &Arc<Run>) -> Self {
            Self {
                run: run.clone(),
                events: Mutex::new(Vec::new()),
                live: Mutex::new(None),
                fail: None,
                cancel_at: None,
            }
        }

        async fn event(&self, phase: &'static str, id: &str) -> Result<()> {
            self.events.lock().unwrap().push(format!("{phase}:{id}"));
            if self.cancel_at == Some(phase) {
                self.run.stop.send_replace(true);
                std::future::pending::<()>().await;
            }
            if self.fail == Some(phase) && (id == "uuid-2" || phase == "stop") {
                bail!("{phase} failed");
            }
            Ok(())
        }
    }

    impl Lifecycle for Fake {
        fn start<'a>(&'a self, id: &'a str, _url: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                assert!(self.live.lock().unwrap().replace(id.into()).is_none());
                self.event("start", id).await
            })
        }

        fn navigate<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(self.event("navigate", id))
        }

        fn stop<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                self.event("stop", id).await?;
                self.live.lock().unwrap().take();
                Ok(())
            })
        }

        fn running(&self, id: &str) -> bool {
            self.live.lock().unwrap().as_deref() == Some(id)
        }
    }

    #[tokio::test]
    async fn all_fifty_are_strictly_sequential() {
        let run = Arc::new(Run::new(50));
        let fake = Fake::new(&run);
        let mut c = config();
        let plan = resolve(&c, &profiles()).unwrap();
        c.wait_seconds = 0; // No wall-clock delay in the lifecycle test.
        drive(run.clone(), c, plan, &fake).await;
        assert_eq!(run.state.lock().unwrap().completed, 50);
        let events = fake.events.lock().unwrap();
        for i in 1..=50 {
            assert_eq!(
                &events[(i - 1) * 3..i * 3],
                &[
                    format!("start:uuid-{i}"),
                    format!("navigate:uuid-{i}"),
                    format!("stop:uuid-{i}")
                ]
            );
        }
        assert!(fake.live.lock().unwrap().is_none());
    }
    #[tokio::test]
    async fn failures_clean_up_and_continue() {
        for phase in ["start", "navigate"] {
            let run = Arc::new(Run::new(3));
            let mut fake = Fake::new(&run);
            fake.fail = Some(phase);
            let mut c = config();
            c.end = 3;
            let plan = resolve(&c, &profiles()).unwrap();
            c.wait_seconds = 0;
            drive(run.clone(), c, plan, &fake).await;
            let s = run.state.lock().unwrap();
            assert_eq!((s.completed, s.failed), (2, 1));
            let events = fake.events.lock().unwrap();
            let stop = events.iter().position(|e| e == "stop:uuid-2").unwrap();
            let next = events.iter().position(|e| e == "start:uuid-3").unwrap();
            assert!(stop < next);
            assert!(fake.live.lock().unwrap().is_none());
        }
    }
    #[tokio::test]
    async fn cancellation_during_start_and_navigation_closes_owned_child() {
        for phase in ["start", "navigate"] {
            let run = Arc::new(Run::new(50));
            let mut fake = Fake::new(&run);
            fake.cancel_at = Some(phase);
            let c = config();
            let plan = resolve(&c, &profiles()).unwrap();
            drive(run.clone(), c, plan, &fake).await;
            assert!(fake.live.lock().unwrap().is_none());
            assert!(!fake
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e == "start:uuid-2"));
            assert_eq!(run.state.lock().unwrap().phase, "stopped");
        }
    }
    #[tokio::test]
    async fn cancellation_during_wait_and_before_start() {
        let run = Arc::new(Run::new(50));
        let fake = Fake::new(&run);
        let c = config();
        let plan = resolve(&c, &profiles()).unwrap();
        let stop = async {
            loop {
                if run.state.lock().unwrap().phase.starts_with("waiting") {
                    run.stop.send_replace(true);
                    break;
                }
                tokio::task::yield_now().await;
            }
        };
        tokio::join!(drive(run.clone(), c.clone(), plan.clone(), &fake), stop);
        assert!(fake.live.lock().unwrap().is_none());
        assert_eq!(run.state.lock().unwrap().results.len(), 1);
        let run = Arc::new(Run::new(50));
        run.stop.send_replace(true);
        let fake = Fake::new(&run);
        drive(run, c, plan, &fake).await;
        assert!(fake.events.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn failed_cleanup_halts_instead_of_overlapping() {
        let run = Arc::new(Run::new(50));
        let mut fake = Fake::new(&run);
        fake.fail = Some("stop");
        let mut c = config();
        let plan = resolve(&c, &profiles()).unwrap();
        c.wait_seconds = 0;
        drive(run.clone(), c, plan, &fake).await;
        let s = run.state.lock().unwrap();
        assert_eq!(s.phase, "cleanup failed");
        assert!(s.error.is_some());
        assert_eq!(s.results.len(), 1);
    }
    #[tokio::test]
    async fn timeout_is_bounded() {
        let (_tx, mut rx) = watch::channel(false);
        let r: Result<()> = step(
            &mut rx,
            Duration::from_millis(1),
            "startup",
            std::future::pending(),
        )
        .await;
        assert!(r.unwrap_err().to_string().contains("startup timed out"));
    }

    #[cfg(feature = "automation")]
    #[tokio::test]
    async fn page_report_accepts_only_the_current_helper_once() -> Result<()> {
        let (mut report, probe) = PageReport::listen("https://example.com/").await?;
        let endpoint = format!("http://{}/{}?outcome=ok", probe.addr, probe.nonce);
        let helper = format!("chrome-extension://{}", crate::startup_page::ID);
        let client = reqwest::Client::builder().no_proxy().build()?;
        assert_eq!(
            client
                .post(&endpoint)
                .header("Origin", "https://example.com")
                .send()
                .await?
                .status(),
            403
        );
        assert_eq!(
            client
                .post(format!("http://{}/stale", probe.addr))
                .header("Origin", &helper)
                .send()
                .await?
                .status(),
            404
        );
        assert!(matches!(
            report.receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(
            client
                .post(&endpoint)
                .header("Origin", &helper)
                .send()
                .await?
                .status(),
            204
        );
        assert_eq!(
            client
                .post(&endpoint)
                .header("Origin", &helper)
                .send()
                .await?
                .status(),
            410
        );
        (&mut report.receiver).await??;
        Ok(())
    }
    #[tokio::test]
    async fn custom_wait_happens_after_navigation_before_stop() {
        let run = Arc::new(Run::new(1));
        let fake = Fake::new(&run);
        let mut c = config();
        c.end = 1;
        c.wait_seconds = 1;
        let plan = resolve(&c, &profiles()).unwrap();
        let t = std::time::Instant::now();
        drive(run, c, plan, &fake).await;
        assert!(t.elapsed() >= Duration::from_secs(1));
        assert!(fake.live.lock().unwrap().is_none());
    }
}

#[cfg(all(test, feature = "automation"))]
mod browser_smoke {
    use super::*;
    /// Opt-in: run this test ALONE with SHARDX_TEST_BROWSER pointing to ShardX.
    /// Uses a disposable test store; the Windows Portable is verified separately.
    #[tokio::test]
    #[ignore = "requires SHARDX_TEST_BROWSER and an isolated test process"]
    async fn real_engine_launch_navigate_stop() -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let browser = std::env::var("SHARDX_TEST_BROWSER").context("set SHARDX_TEST_BROWSER")?;
        let store = crate::store::test_root("sequential-engine");
        let root = &store.path;
        let result = async {
            assert_eq!(crate::store::data_root()?, *root);
            let mut settings = crate::settings::load()?;
            settings.browser_path = Some(browser);
            settings.extra_args = "--start-minimized --disable-gpu".into();
            crate::settings::save(&settings)?;
            let mut ids = vec![];
            for n in 1..=2 {
                let mut p = crate::space::profile::StoredProfile::default();
                p.meta.id = uuid::Uuid::new_v4().to_string();
                p.config.insert("name".into(), serde_json::json!(format!("{n:03}")));
                crate::space::profile::save_raw(&mut p)?;
                ids.push(p.meta.id);
            }
            let server = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let addr = server.local_addr()?;
            let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = hits.clone();
            let serving = tokio::spawn(async move {
                while let Ok((mut stream,_)) = server.accept().await {
                    let counter = counter.clone();
                    tokio::spawn(async move {
                        let mut buf = [0;4096]; let n=stream.read(&mut buf).await.unwrap_or(0);
                        if String::from_utf8_lossy(&buf[..n]).starts_with("GET /test ") { counter.fetch_add(1,std::sync::atomic::Ordering::SeqCst); }
                        let body="<!doctype html><title>Sequential test</title><p>Ready</p>";
                        let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                        let _=stream.write_all(response.as_bytes()).await;
                    });
                }
            });
            let cfg=Config {start:1,end:2,url:format!("http://{addr}/test"),wait_seconds:1};
            let batch = async {
                start(cfg).await?;
                tokio::time::timeout(Duration::from_secs(240), async { while active() { tokio::time::sleep(POLL).await; } }).await?;
                let s=status();
                anyhow::ensure!(s.completed==2 && s.failed==0,"batch did not complete: {s:?}");
                anyhow::ensure!(crate::process::Tracker::shared().running().is_empty(),"profile left running");
                anyhow::ensure!(hits.load(std::sync::atomic::Ordering::SeqCst)>=2,"target page was not requested by both profiles");
                for id in &ids { anyhow::ensure!(root.join("user-data").join(id).join("Default/Preferences").exists(),"profile not saved in the test store"); }
                start(Config {start:1,end:2,url:format!("http://{addr}/test"),wait_seconds:60}).await?;
                tokio::time::timeout(Duration::from_secs(120), async {
                    while active() && !status().phase.starts_with("waiting") { tokio::time::sleep(POLL).await; }
                }).await?;
                anyhow::ensure!(active(), "batch exited before cancellation: {:?}", status());
                anyhow::ensure!(status().current_id.as_deref() == Some(ids[0].as_str()), "cancellation sample did not stay on the first profile");
                stop_and_wait().await?;
                anyhow::ensure!(status().phase=="stopped" && status().results.len()==1,"Stop advanced to another profile: {:?}",status());
                anyhow::ensure!(crate::process::Tracker::shared().running().is_empty(),"Stop left a child running");
                Ok::<(),anyhow::Error>(())
            }.await;
            serving.abort();
            batch
        }.await;
        stop_and_wait().await?;
        result
    }
}
