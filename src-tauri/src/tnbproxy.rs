//! TNB's IP-allowlisted extraction API. Only explicit user actions call /gen;
//! returned entries remain previews until the existing bulk-save command runs.
use crate::proxy_chain::{connector::Target, tunnel::Tunnel};
use crate::space::proxy::{self, ProxyEntry, ProxyKind};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::Semaphore;
use url::Url;

const MAX_BODY: usize = 512 * 1024;
const TIMEOUT: Duration = Duration::from_secs(25);
static REQUESTS: Semaphore = Semaphore::const_new(2);
static LOCATION_REQUESTS: Semaphore = Semaphore::const_new(4);
const LOCATIONS_SOURCE: &str = "https://server.tnbproxy.com/api/regions/list";

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route { Direct, FirstHop }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractRequest {
    pub server: String,
    pub plan_name: String,
    pub kind: String,
    pub count: u16,
    #[serde(default)]
    pub geo: String,
    pub ttl: u16,
    #[serde(default)]
    pub pre: bool,
    pub route: Route,
    pub pool: Option<String>,
    pub isp: Option<String>,
    pub asn: Option<String>,
    pub ip: Option<String>,
    pub sid: Option<String>,
}

#[derive(Serialize)]
pub struct Extraction {
    pub entries: Vec<ProxyEntry>,
    pub route: Route,
    pub server: String,
    pub requested_count: u16,
}

type Result<T> = std::result::Result<T, &'static str>;

fn request_url(request: &ExtractRequest) -> Result<Url> {
    if !(1..=1000).contains(&request.count) || request.ttl > 180
        || !matches!(request.kind.as_str(), "http" | "socks5")
        || request.plan_name.is_empty() || request.plan_name.len() > 64
        || !request.plan_name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
    { return Err("tnb_invalid_request"); }
    let mut url = server_url(&request.server)?;
    url.set_path("/gen");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("plan_name", &request.plan_name)
            .append_pair("count", &request.count.to_string())
            .append_pair("format", "txt").append_pair("split", "rn")
            .append_pair("ttl", &request.ttl.to_string());
        for (key, value) in [
            ("geo", Some(request.geo.as_str())), ("pool", request.pool.as_deref()),
            ("isp", request.isp.as_deref()), ("asn", request.asn.as_deref()),
            ("ip", request.ip.as_deref()), ("sid", request.sid.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                if value.len() > 256 || value.chars().any(char::is_control) { return Err("tnb_invalid_request"); }
                query.append_pair(key, value);
            }
        }
        if request.pre { query.append_pair("pre", "pre"); }
    }
    Ok(url)
}

fn server_url(raw: &str) -> Result<Url> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return Err("tnb_invalid_request");
    }
    let url = Url::parse(raw.trim()).map_err(|_| "tnb_invalid_request")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some()
        || !matches!(url.path(), "" | "/" | "/gen") || url.port() == Some(0)
    { return Err("tnb_invalid_request"); }
    Ok(url)
}

fn origin(url: &Url) -> String { url.origin().ascii_serialization() }

fn parse_response(body: &[u8], request: &ExtractRequest, server: &str) -> Result<Vec<ProxyEntry>> {
    let body = std::str::from_utf8(body).map_err(|_| "tnb_invalid_response")?;
    let kind = if request.kind == "http" { ProxyKind::Http } else { ProxyKind::Socks5 };
    let lines: Vec<&str> = body.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() { return Err("tnb_no_proxies"); }
    if lines.len() > usize::from(request.count) { return Err("tnb_invalid_response"); }
    let mut entries = Vec::with_capacity(lines.len());
    for line in lines {
        if line.len() > 4096 || line.contains('#') || line.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err("tnb_invalid_response");
        }
        // The generic importer treats an unknown scheme as userinfo. Reject
        // explicit protocol mismatches before it can turn socks4://u:p into
        // an HTTP username/password; :// inside a raw password stays valid.
        if let Some((scheme, _)) = line.split_once("://") {
            if scheme.as_bytes().first().is_some_and(|c| c.is_ascii_alphabetic())
                && scheme.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
                && scheme != request.kind
            { return Err("tnb_invalid_response"); }
        }
        let mut parsed = proxy::parse_bulk(line, kind.clone());
        let Some(mut entry) = parsed.pop().filter(|_| parsed.is_empty()) else { return Err("tnb_invalid_response"); };
        // A provider error such as "error:403" is not a host. The extraction
        // contract returns IPs or fully-qualified hosts, never prose/comments.
        if entry.kind != kind || entry.port == 0
            || (!entry.host.contains('.') && !entry.host.contains(':'))
            || Target::new(&entry.host, entry.port).is_err()
            || entry.username.len() + entry.password.len() > 4096
            || entry.username.chars().chain(entry.password.chars()).any(char::is_control)
        { return Err("tnb_invalid_response"); }
        entry.name = format!("TNBProxy {}:{}", entry.host, entry.port);
        // Reuses the durable notes field, which already survives edit/export/
        // team sync. No change/clean URL is guessed and no management call runs.
        entry.notes = format!("TNBProxy\n{}", serde_json::json!({
            "server": server, "plan_name": request.plan_name, "sid": request.sid,
            "ttl": request.ttl, "route": request.route,
        }));
        entries.push(entry);
    }
    Ok(entries)
}

fn request_error(error: reqwest::Error) -> &'static str {
    if error.is_timeout() { "tnb_timeout" } else { "tnb_request_failed" }
}

async fn download(client: reqwest::Client, url: Url, host: Option<String>, json_body: Option<serde_json::Value>) -> Result<Vec<u8>> {
    let metadata = json_body.is_some();
    let mut request = if let Some(body) = json_body {
        client.post(url).json(&body).header(reqwest::header::ACCEPT, "application/json")
    } else { client.get(url).header(reqwest::header::ACCEPT, "text/plain") };
    if let Some(host) = host { request = request.header(reqwest::header::HOST, host); }
    let mut response = request.send().await.map_err(request_error)?;
    if metadata && matches!(response.status().as_u16(), 401 | 403) { return Err("tnb_auth_required"); }
    if !response.status().is_success() { return Err("tnb_http_status"); }
    if response.content_length().is_some_and(|n| n > MAX_BODY as u64) { return Err("tnb_response_too_large"); }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY { return Err("tnb_response_too_large"); }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub async fn extract(request: ExtractRequest) -> Result<Extraction> {
    extract_with_timeout(request, TIMEOUT).await
}

async fn extract_with_timeout(request: ExtractRequest, timeout: Duration) -> Result<Extraction> {
    let _slot = REQUESTS.try_acquire().map_err(|_| "tnb_busy")?;
    let url = request_url(&request)?;
    let server = origin(&url);
    let body = fetch_on_route(url, request.route, None, timeout).await?;
    let entries = parse_response(&body, &request, &server)?;
    Ok(Extraction { entries, route: request.route, server, requested_count: request.count })
}

async fn fetch_on_route(mut url: Url, route: Route, json_body: Option<serde_json::Value>, timeout: Duration) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none()).http1_only()
        .pool_max_idle_per_host(0).connect_timeout(Duration::from_secs(8))
        .timeout(timeout).build().map_err(|_| "tnb_request_failed")?;
    let mut tunnel = None;
    let fetched = tokio::time::timeout(timeout, async {
        let mut host = None;
        if matches!(route, Route::FirstHop) {
            let target = Target::new(url.host_str().ok_or("tnb_invalid_request")?, url.port_or_known_default().ok_or("tnb_invalid_request")?).map_err(|_| "tnb_invalid_request")?;
            let saved = crate::proxy_chain::saved_settings().map_err(|_| "tnb_route_failed")?;
            let hop = saved.proxy_chain.hop_for(&saved.proxy_chain, &target).await.map_err(|_| "tnb_route_failed")?;
            // This transparent endpoint has one fixed destination. HTTPS is
            // verified inside the tunnel against that original host, never
            // against localhost and never with certificate checks disabled.
            let bound = if url.scheme() == "https" {
                let tls = crate::proxy_chain::tls::client().map_err(|_| "tnb_route_failed")?;
                Tunnel::bind_tls(hop, target, tls).await
            } else { Tunnel::bind(hop, target).await }.map_err(|_| "tnb_route_failed")?;
            host = Some(url[url::Position::BeforeHost..url::Position::AfterPort].to_owned());
            url.set_scheme("http").map_err(|_| "tnb_invalid_request")?;
            url.set_host(Some("127.0.0.1")).map_err(|_| "tnb_invalid_request")?;
            url.set_port(Some(bound.addr().port())).map_err(|_| "tnb_invalid_request")?;
            tunnel = Some(bound);
        }
        download(client, url, host, json_body).await
    }).await.map_err(|_| "tnb_timeout").and_then(|r| r);
    if let Some(tunnel) = tunnel {
        let failed = tunnel.last_error().is_some();
        tunnel.close_and_wait().await.map_err(|_| "tnb_route_failed")?;
        if failed && fetched.is_err() { return Err("tnb_route_failed"); }
    }
    fetched
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationLevel { Country, State, City }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationsRequest {
    pub server: String,
    pub route: Route,
    pub level: LocationLevel,
    pub country: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LocationItem { pub code: String, pub label: String }

#[derive(Debug, Serialize)]
pub struct Locations {
    pub items: Vec<LocationItem>,
    pub supported: bool,
    pub source: &'static str,
}

fn locations_error(code: &'static str) -> &'static str {
    match code {
        "tnb_invalid_request" => "tnb_locations_invalid_request",
        "tnb_timeout" => "tnb_locations_timeout",
        "tnb_route_failed" => "tnb_locations_route_failed",
        "tnb_http_status" => "tnb_locations_http_status",
        "tnb_auth_required" => "tnb_locations_auth_required",
        "tnb_response_too_large" => "tnb_locations_too_large",
        _ => "tnb_locations_request_failed",
    }
}

fn locations_body(request: &LocationsRequest) -> Result<serde_json::Value> {
    let country = request.country.as_deref().unwrap_or("");
    let state = request.state.as_deref().unwrap_or("");
    if (!country.is_empty() && (country.len() != 2 || !country.bytes().all(|c| c.is_ascii_uppercase())))
        || state.len() > 128 || state.chars().any(char::is_control)
        || state != state.trim()
    { return Err("tnb_locations_invalid_request"); }
    match request.level {
        LocationLevel::Country if country.is_empty() && state.is_empty() => Ok(serde_json::json!({})),
        LocationLevel::State if !country.is_empty() && state.is_empty() => Ok(serde_json::json!({"region":country})),
        LocationLevel::City if !country.is_empty() && !state.is_empty() => Ok(serde_json::json!({"region":country,"state":state})),
        _ => Err("tnb_locations_invalid_request"),
    }
}

fn parse_locations(bytes: &[u8], request: &LocationsRequest) -> Result<Vec<LocationItem>> {
    let response: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "tnb_locations_invalid_response")?;
    if matches!(response.get("code").and_then(serde_json::Value::as_i64), Some(401 | 403)) { return Err("tnb_locations_auth_required"); }
    if response.get("code").and_then(serde_json::Value::as_i64) != Some(0) { return Err("tnb_locations_invalid_response"); }
    let rows = response.get("data").and_then(serde_json::Value::as_array).ok_or("tnb_locations_invalid_response")?;
    if rows.len() > 10_000 { return Err("tnb_locations_too_large"); }
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let region = row.get("region").and_then(serde_json::Value::as_str).ok_or("tnb_locations_invalid_response")?;
        let state = row.get("state").and_then(serde_json::Value::as_str).ok_or("tnb_locations_invalid_response")?;
        let city = row.get("city").and_then(serde_json::Value::as_str).ok_or("tnb_locations_invalid_response")?;
        if (!region.is_empty() && (region.len() != 2 || !region.bytes().all(|c| c.is_ascii_uppercase())))
            || [state, city].iter().any(|value| value.len() > 128 || value.chars().any(char::is_control) || *value != value.trim())
        { return Err("tnb_locations_invalid_response"); }
        if !matches!(request.level, LocationLevel::Country) && Some(region) != request.country.as_deref() { return Err("tnb_locations_invalid_response"); }
        if matches!(request.level, LocationLevel::City) && Some(state) != request.state.as_deref() { return Err("tnb_locations_invalid_response"); }
        let code = match request.level { LocationLevel::Country => region, LocationLevel::State => state, LocationLevel::City => city };
        // The provider includes a blank choice for "any" at each level. The
        // UI owns its translated "any" row; aliases/case otherwise stay exact.
        if !code.is_empty() && seen.insert(code.to_owned()) {
            items.push(LocationItem { code: code.to_owned(), label: code.to_owned() });
        }
    }
    Ok(items)
}

/// Public metadata only, with no account token, cookies, device id or /gen.
pub async fn locations(request: LocationsRequest) -> Result<Locations> {
    let endpoint = Url::parse(LOCATIONS_SOURCE).map_err(|_| "tnb_locations_request_failed")?;
    locations_using(request, endpoint, Duration::from_secs(20)).await
}

async fn locations_using(request: LocationsRequest, endpoint: Url, timeout: Duration) -> Result<Locations> {
    let server = server_url(&request.server).map_err(locations_error)?;
    let body = locations_body(&request)?;
    let host = server.host_str().ok_or("tnb_locations_invalid_request")?;
    if host != "tnbproxy.com" && !host.ends_with(".tnbproxy.com") {
        return Ok(Locations { items: Vec::new(), supported: false, source: "" });
    }
    let _slot = LOCATION_REQUESTS.try_acquire().map_err(|_| "tnb_locations_busy")?;
    let bytes = fetch_on_route(endpoint, request.route, Some(body), timeout).await.map_err(locations_error)?;
    Ok(Locations { items: parse_locations(&bytes, &request)?, supported: true, source: LOCATIONS_SOURCE })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

    fn request(server: String) -> ExtractRequest {
        ExtractRequest { server, plan_name: "advanced_traffic".into(), kind: "http".into(), count: 10, geo: "us_california_los angeles".into(), ttl: 15, pre: true, route: Route::Direct, pool: None, isp: None, asn: None, ip: None, sid: Some("fixture-session".into()) }
    }

    async fn read_head(stream: &mut TcpStream) -> String {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let byte = stream.read_u8().await.unwrap();
            head.push(byte);
            assert!(head.len() < 16 * 1024);
        }
        String::from_utf8(head).unwrap()
    }

    async fn fixture(reply: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let head = read_head(&mut stream).await;
            // A rejected oversized response may be closed before the whole
            // fixture is consumed; that early close is the behavior under test.
            let _ = stream.write_all(reply.as_bytes()).await;
            head
        });
        (server, task)
    }

    fn ok(body: &str) -> String { format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()) }

    fn locations_request(level: LocationLevel, country: Option<&str>, state: Option<&str>) -> LocationsRequest {
        LocationsRequest { server: "http://usdata.adv.tnbproxy.com:9090".into(), route: Route::Direct, level, country: country.map(str::to_owned), state: state.map(str::to_owned) }
    }

    async fn metadata_fixture(reply: String) -> (Url, tokio::task::JoinHandle<(String, serde_json::Value)>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}/api/regions/list", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let head = read_head(&mut stream).await;
            let size = head.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap();
            assert!(size < 1024);
            let mut body = vec![0u8; size];
            stream.read_exact(&mut body).await.unwrap();
            let _ = stream.write_all(reply.as_bytes()).await;
            (head, serde_json::from_slice(&body).unwrap())
        });
        (endpoint, task)
    }

    #[tokio::test]
    async fn tnb_locations_use_public_post_contract_and_preserve_provider_codes() {
        use serde_json::json;
        for (level, country, state, payload, expected_body, expected_codes) in [
            (LocationLevel::Country, None, None, json!([{"id":0,"region":"","state":"","city":""},{"id":1,"region":"EG","state":"","city":""}]), json!({}), vec!["EG"]),
            (LocationLevel::State, Some("EG"), None, json!([{"id":0,"region":"EG","state":"","city":""},{"id":1,"region":"EG","state":"Cairo","city":""},{"id":2,"region":"EG","state":"AlQahirah","city":""},{"id":3,"region":"EG","state":"Cairo","city":""}]), json!({"region":"EG"}), vec!["Cairo","AlQahirah"]),
            (LocationLevel::City, Some("EG"), Some("Cairo"), json!([{"id":0,"region":"EG","state":"Cairo","city":""},{"id":1,"region":"EG","state":"Cairo","city":"Cairo"},{"id":2,"region":"EG","state":"Cairo","city":"New-Cairo"}]), json!({"region":"EG","state":"Cairo"}), vec!["Cairo","New-Cairo"]),
        ] {
            let (endpoint, task) = metadata_fixture(ok(&json!({"code":0,"data":payload}).to_string())).await;
            let result = locations_using(locations_request(level, country, state), endpoint, TIMEOUT).await.unwrap();
            assert!(result.supported);
            assert_eq!(result.source, LOCATIONS_SOURCE);
            assert_eq!(result.items.iter().map(|item| item.code.as_str()).collect::<Vec<_>>(), expected_codes);
            let (head, body) = task.await.unwrap();
            assert!(head.starts_with("POST /api/regions/list HTTP/1.1\r\n"));
            let head = head.to_ascii_lowercase();
            assert!(head.contains("content-type: application/json"));
            assert!(!head.contains("authorization:") && !head.contains("cookie:") && !head.contains("device"));
            assert_eq!(body, expected_body);
        }
    }

    #[tokio::test]
    async fn tnb_locations_reject_invalid_parents_and_do_not_query_custom_servers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}/api/regions/list", listener.local_addr().unwrap())).unwrap();
        for (level, country, state) in [
            (LocationLevel::Country, Some("EG"), None),
            (LocationLevel::State, None, None),
            (LocationLevel::State, Some("eg"), None),
            (LocationLevel::City, Some("EG"), None),
            (LocationLevel::City, Some("EG"), Some("Cairo\nInjected")),
        ] {
            assert_eq!(locations_using(locations_request(level, country, state), endpoint.clone(), TIMEOUT).await.unwrap_err(), "tnb_locations_invalid_request");
        }
        for server in ["http://127.0.0.1:9090", "https://tnbproxy.com.evil.test", "https://eviltnbproxy.com"] {
            let mut request = locations_request(LocationLevel::Country, None, None);
            request.server = server.into();
            let result = locations_using(request, endpoint.clone(), TIMEOUT).await.unwrap();
            assert!(!result.supported && result.items.is_empty() && result.source.is_empty());
        }
        assert!(tokio::time::timeout(Duration::from_millis(50), listener.accept()).await.is_err());
    }

    #[tokio::test]
    async fn tnb_locations_errors_do_not_echo_account_or_response_text() {
        for (reply, error) in [
            ("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".into(), "tnb_locations_auth_required"),
            ("HTTP/1.1 302 Found\r\nLocation: https://other.invalid/\r\nContent-Length: 0\r\n\r\n".into(), "tnb_locations_http_status"),
            (ok(r#"{"code":401,"message":"secret-account"}"#), "tnb_locations_auth_required"),
            (ok(r#"{"code":0,"data":[{"region":"US","state":"Wrong parent","city":""}]}"#), "tnb_locations_invalid_response"),
            (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1), "tnb_locations_too_large"),
        ] {
            let (endpoint, task) = metadata_fixture(reply).await;
            assert_eq!(locations_using(locations_request(LocationLevel::State, Some("EG"), None), endpoint, TIMEOUT).await.unwrap_err(), error);
            task.await.unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Url::parse(&format!("http://{}/api/regions/list", listener.local_addr().unwrap())).unwrap();
        assert_eq!(locations_using(locations_request(LocationLevel::Country, None, None), endpoint, Duration::from_millis(60)).await.unwrap_err(), "tnb_locations_timeout");
    }

    #[tokio::test]
    #[ignore = "Read-only public metadata; run explicitly, never extracts proxies or reads account credentials"]
    async fn tnb_locations_public_egypt_contract_live() {
        let countries = locations(locations_request(LocationLevel::Country, None, None)).await.unwrap();
        assert!(countries.items.iter().any(|item| item.code == "EG"));
        let states = locations(locations_request(LocationLevel::State, Some("EG"), None)).await.unwrap();
        assert!(states.items.iter().any(|item| item.code == "Cairo"));
        let cities = locations(locations_request(LocationLevel::City, Some("EG"), Some("Cairo"))).await.unwrap();
        assert!(cities.items.iter().any(|item| item.code == "Cairo"));
        println!("TNB public metadata: countries={}, EG states={}, Cairo cities={}, source={LOCATIONS_SOURCE}; no authentication or extraction", countries.items.len(), states.items.len(), cities.items.len());
    }

    #[test]
    fn tnb_request_encoding_bounds_and_response_validation() {
        let mut r = request("http://usdata.adv.tnbproxy.com:9090/gen".into());
        r.sid = Some("x&count=999".into());
        let url = request_url(&r).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["count"], "10");
        assert_eq!(query["format"], "txt");
        assert_eq!(query["split"], "rn");
        assert_eq!(query["sid"], "x&count=999");
        assert_eq!(query["pre"], "pre");
        for server in ["file:///tmp/gen", "http://u:p@host/gen", "http://host:0/gen", "http://host/change", "http://host/?count=99", "http://host/#token"] {
            r.server = server.into();
            assert_eq!(request_url(&r).unwrap_err(), "tnb_invalid_request");
        }
        r.server = "https://example.com/".into();
        r.count = 0;
        assert_eq!(request_url(&r).unwrap_err(), "tnb_invalid_request");
        r.count = 1001;
        assert_eq!(request_url(&r).unwrap_err(), "tnb_invalid_request");
        r.count = 1;
        r.ttl = 181;
        assert_eq!(request_url(&r).unwrap_err(), "tnb_invalid_request");
        for body in ["error:403", "{\"error\":\"secret password\"}", "<html>denied</html>", "1.2.3.4:0", "1.2.3.4:80\nerror", "1.2.3.4:80\n2.3.4.5:81", "socks5://1.2.3.4:80"] {
            assert_eq!(parse_response(body.as_bytes(), &r, "https://example.com").unwrap_err(), "tnb_invalid_response");
        }
        assert_eq!(parse_response(b"\r\n", &r, "https://example.com").unwrap_err(), "tnb_no_proxies");
    }

    #[test]
    fn tnb_explicit_protocols_cannot_be_parsed_as_credentials() {
        let mut r = request("https://example.com".into());
        for kind in ["http", "socks5"] {
            r.kind = kind.into();
            for scheme in ["socks4", "ftp", "https", if kind == "http" { "socks5" } else { "http" }] {
                let body = format!("{scheme}://u:p@198.51.100.9:1080");
                assert_eq!(parse_response(body.as_bytes(), &r, "https://example.com").unwrap_err(), "tnb_invalid_response", "{kind}: {scheme}");
            }
            for body in [format!("{kind}://u:p@198.51.100.9:1080"), "u:p://tail@198.51.100.9:1080".into()] {
                let entries = parse_response(body.as_bytes(), &r, "https://example.com").unwrap();
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].username, "u");
                assert_eq!(entries[0].password, if body.starts_with(kind) { "p" } else { "p://tail" });
            }
        }
    }

    #[tokio::test]
    async fn tnb_direct_extracts_a_preview_without_saving_and_uses_expected_query() {
        let _root = crate::store::test_root("tnb-direct");
        let (server, task) = fixture(ok("198.51.100.7:1234:user:synthetic-pass\r\n203.0.113.9:5678\r\n")).await;
        let result = extract(request(server.clone())).await.unwrap();
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].password, "synthetic-pass");
        assert_eq!(result.entries[0].kind, ProxyKind::Http);
        assert_eq!(result.server, server);
        assert!(result.entries[0].notes.contains("fixture-session"));
        assert!(!crate::store::proxies_path().unwrap().exists());
        let head = task.await.unwrap();
        assert!(head.starts_with("GET /gen?plan_name=advanced_traffic&count=10&format=txt&split=rn&ttl=15&geo=us_california_los+angeles&sid=fixture-session&pre=pre HTTP/1.1"), "{head}");
    }

    #[tokio::test]
    async fn tnb_rejects_redirect_errors_oversized_bodies_and_timeouts() {
        let chunk = "x".repeat(MAX_BODY + 1);
        for (reply, error) in [
            ("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/gen\r\nContent-Length: 0\r\n\r\n".into(), "tnb_http_status"),
            ("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".into(), "tnb_http_status"),
            (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1), "tnb_response_too_large"),
            (format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{chunk}\r\n0\r\n\r\n", chunk.len()), "tnb_response_too_large"),
            (ok("whitelist denied: secret credential"), "tnb_invalid_response"),
        ] {
            let (server, task) = fixture(reply).await;
            assert_eq!(extract(request(server)).await.err(), Some(error));
            task.await.unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        assert_eq!(extract_with_timeout(request(server), Duration::from_millis(60)).await.err(), Some("tnb_timeout"));
    }

    #[tokio::test]
    async fn tnb_first_hop_uses_connect_and_never_falls_back_when_refused() {
        let _root = crate::store::test_root("tnb-first-hop");
        let mut saved = crate::settings::load().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        saved.proxy_chain.mode = "http".into();
        saved.proxy_chain.host = "127.0.0.1".into();
        saved.proxy_chain.port = listener.local_addr().unwrap().port();
        crate::settings::save(&saved).unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let connect = read_head(&mut stream).await;
            stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.unwrap();
            let get = read_head(&mut stream).await;
            stream.write_all(ok("198.51.100.1:1234").as_bytes()).await.unwrap();
            (connect, get)
        });
        let mut r = request("http://provider.invalid:9090".into());
        r.route = Route::FirstHop;
        let result = extract(r).await.unwrap();
        assert_eq!(result.entries.len(), 1);
        let (connect, get) = task.await.unwrap();
        assert!(connect.starts_with("CONNECT provider.invalid:9090 HTTP/1.1\r\n"));
        assert!(get.to_lowercase().contains("host: provider.invalid:9090\r\n"));
        let (hop, task) = fixture("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".into()).await;
        saved.proxy_chain.port = Url::parse(&hop).unwrap().port().unwrap();
        crate::settings::save(&saved).unwrap();
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut r = request(format!("http://{}", target.local_addr().unwrap()));
        r.route = Route::FirstHop;
        assert_eq!(extract(r).await.err(), Some("tnb_route_failed"));
        assert!(task.await.unwrap().starts_with("CONNECT "));
        assert!(tokio::time::timeout(Duration::from_millis(50), target.accept()).await.is_err());
    }

    #[tokio::test]
    async fn tnb_https_through_first_hop_rejects_untrusted_certificates() {
        use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
        use std::sync::Arc;
        let _root = crate::store::test_root("tnb-tls");
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions().unwrap().with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(certified.cert.der().to_vec())], PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()).into()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut saved = crate::settings::load().unwrap();
        saved.proxy_chain.mode = "http".into();
        saved.proxy_chain.host = "127.0.0.1".into();
        saved.proxy_chain.port = listener.local_addr().unwrap().port();
        crate::settings::save(&saved).unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let connect = read_head(&mut stream).await;
            assert!(connect.starts_with("CONNECT 127.0.0.1:443 HTTP/1.1"));
            stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.unwrap();
            assert!(tokio_rustls::TlsAcceptor::from(Arc::new(tls)).accept(stream).await.is_err());
        });
        let mut r = request("https://127.0.0.1".into());
        r.route = Route::FirstHop;
        assert_eq!(extract(r).await.err(), Some("tnb_route_failed"));
        task.await.unwrap();
    }
}
