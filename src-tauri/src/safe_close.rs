use crate::{launch, process::Tracker, space};
use std::sync::atomic::{AtomicU8, Ordering::SeqCst};
use std::time::Duration;

const IDLE: u8 = 0;
const CLOSING: u8 = 1;
const READY: u8 = 2;
const CANCELLED: u8 = 3;
const SAVE_FAILED: u8 = 4;
static STATE: AtomicU8 = AtomicU8::new(IDLE);

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    Busy,
    Cancelled,
    TimedOut,
    SaveFailed,
    Unsealed,
}

impl Failure {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Busy => "Finish the automation, profile launch, or data move before quitting.",
            Self::Cancelled => "Exit cancelled.",
            Self::TimedOut => {
                "Some profiles are still closing or saving. Check their status and try again."
            }
            Self::SaveFailed => {
                "A browser session could not be saved. Check the session status before quitting."
            }
            Self::Unsealed => {
                "A protected profile is still unsealed. Open and close it again before quitting."
            }
        }
    }
}

pub(crate) fn begin() -> bool {
    STATE
        .compare_exchange(IDLE, CLOSING, SeqCst, SeqCst)
        .is_ok()
}

pub(crate) fn active() -> bool {
    STATE.load(SeqCst) != IDLE
}

pub(crate) fn ready() -> bool {
    STATE.load(SeqCst) == READY
}

pub(crate) fn cancel() {
    let _ = STATE.compare_exchange(CLOSING, CANCELLED, SeqCst, SeqCst);
}

pub(crate) fn save_failed() {
    let _ = STATE.compare_exchange(CLOSING, SAVE_FAILED, SeqCst, SeqCst);
}

pub(crate) async fn finish() -> Result<(), Failure> {
    let result = drain().await.and_then(|()| {
        STATE
            .compare_exchange(CLOSING, READY, SeqCst, SeqCst)
            .map(|_| ())
            .map_err(|state| {
                if state == CANCELLED {
                    Failure::Cancelled
                } else {
                    Failure::SaveFailed
                }
            })
    });
    if result.is_err() {
        STATE.store(IDLE, SeqCst);
    }
    result
}

async fn drain() -> Result<(), Failure> {
    {
        let _gate = crate::sequential::launch_gate().lock().await;
        #[cfg(feature = "automation")]
        if crate::runner::all().iter().any(|run| run.running) {
            return Err(Failure::Busy);
        }
        if crate::sequential::active()
            || launch::in_flight() != 0
            || (crate::migrate::moving() && !crate::reset::awaiting_restart())
        {
            return Err(Failure::Busy);
        }
    }

    let tracker = Tracker::shared();
    let done = || tracker.is_idle() && !space::open::any_open() && !space::profile::any_unsealed();
    if !done() {
        let running = tracker.running();
        let requests = futures_util::future::join_all(
            running
                .iter()
                .map(|profile| tracker.request_graceful_close(&profile.profile_id)),
        );
        tokio::pin!(requests);
        let mut started = false;
        let mut reported = None;
        let deadline = tokio::time::Instant::now() + space::open::QUIT_WAIT;
        loop {
            if STATE.load(SeqCst) == CANCELLED {
                return Err(Failure::Cancelled);
            }
            if done() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Failure::TimedOut);
            }
            let remaining = tracker.running().len();
            if reported != Some(remaining) {
                crate::notify_event(
                    "app://safe-close",
                    serde_json::json!({ "remaining": remaining }),
                );
                reported = Some(remaining);
            }
            if !started {
                tokio::select! {
                    _ = &mut requests => started = true,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {},
                }
                continue;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    // The recovery screen starts no profiles. Leave its pending layout alone:
    // looking for unsealed profiles would create a profiles directory.
    if !crate::migrate::reset_pending() && !space::profile::left_open().is_empty() {
        return Err(Failure::Unsealed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        STATE.store(IDLE, SeqCst);
    }

    #[test]
    fn begin_enters_closing_once() {
        let _serial = crate::migrate::serial();
        reset();

        assert!(begin());
        assert!(active());
        assert!(!ready());
        assert!(!begin());

        reset();
    }

    #[test]
    fn cancel_marks_close_and_prevents_ready_transition() {
        let _serial = crate::migrate::serial();
        reset();

        assert!(begin());
        cancel();
        assert!(active());
        assert!(!ready());

        reset();
    }

    #[test]
    fn save_failure_marks_close_and_prevents_ready_transition() {
        let _serial = crate::migrate::serial();
        reset();

        assert!(begin());
        save_failed();
        assert!(active());
        assert!(!ready());

        reset();
    }
}
