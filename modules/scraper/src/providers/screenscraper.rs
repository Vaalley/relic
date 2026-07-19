//! ScreenScraper provider — `https://www.screenscraper.fr/api2` (PLAN.md §7.1).
//!
//! ScreenScraper is a community ROM metadata/media database. Access requires
//! developer credentials (`devid` / `devpassword`); end-user credentials
//! (`ssid` / `sspassword`) are optional but raise the rate-limit tier. Per
//! AGENTS.md hard rule #1 and PLAN.md's T1 tier, this entire file is compiled
//! only behind the `screenscraper` cargo feature — a default build of
//! relic-scraper pulls in no HTTP client and makes no network calls.
//!
//! Field/param names and the exact not-found/error signalling used below are
//! based on ScreenScraper's public API v2 docs as understood at implementation
//! time. Anything not confirmed against the live docs is marked `// VERIFY:`
//! (per the project convention documented in `docs/retroachievements-design.md`:
//! marked uncertainty over confident guessing).

use crate::provider::{Candidate, Provider, ProviderError, SearchQuery};

/// ScreenScraper API v2 base URL (documented host for `jeuInfos.php` etc.).
const DEFAULT_BASE_URL: &str = "https://www.screenscraper.fr/api2";

/// Identifies this client to ScreenScraper (required param `softname`).
/// VERIFY: ScreenScraper asks client authors to register a `softname`; using
/// a stable literal here is the same shape as other frontends (ES-DE, Skraper)
/// do, but a dedicated registration may be needed for higher rate limits.
const SOFTNAME: &str = "relic";

/// A ScreenScraper API v2 client implementing `relic_scraper::provider::Provider`.
///
/// Anonymous mode (no `ssid`/`sspassword`) works but is rate-limited more
/// aggressively; `with_user` raises the tier. `with_base_url` exists primarily
/// so tests can point the client at a local mock server instead of the real
/// API — no test in this file ever touches the network.
pub struct ScreenScraperProvider {
    base_url: String,
    dev_id: String,
    dev_password: String,
    /// Optional end-user credentials (`ssid` / `sspassword` in ScreenScraper's
    /// param naming). `None` = anonymous mode.
    user: Option<(String, String)>,
    http: reqwest::blocking::Client,
}

impl ScreenScraperProvider {
    /// Anonymous mode (no end-user credentials). `dev_id` / `dev_password`
    /// are the developer credentials every ScreenScraper client must supply.
    pub fn new(dev_id: impl Into<String>, dev_password: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            dev_id: dev_id.into(),
            dev_password: dev_password.into(),
            user: None,
            http: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("reqwest blocking client builder has no invalid defaults"),
        }
    }

    /// Builder: attach end-user credentials to use ScreenScraper's registered
    /// rate-limit tier. Consumes and returns `self` for chaining.
    pub fn with_user(mut self, login: impl Into<String>, password: impl Into<String>) -> Self {
        self.user = Some((login.into(), password.into()));
        self
    }

    /// Builder: override the API base URL. The default is ScreenScraper's real
    /// API v2 host; tests pass a `http://127.0.0.1:{port}` URL pointing at a
    /// local mock server so no real network access is ever performed.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Build the query param list common to every ScreenScraper request:
    /// developer creds, softname, and (if present) user creds.
    fn auth_params(&self) -> Vec<(&'static str, String)> {
        // 3 base params + up to 2 user-auth params.
        let mut params: Vec<(&'static str, String)> = Vec::with_capacity(5);
        params.push(("devid", self.dev_id.clone()));
        params.push(("devpassword", self.dev_password.clone()));
        params.push(("softname", SOFTNAME.to_string()));
        if let Some((login, password)) = &self.user {
            // VERIFY: ScreenScraper docs name the user-auth params `ssid` and
            // `sspassword`; confirm against current API v2 docs before shipping.
            params.push(("ssid", login.clone()));
            params.push(("sspassword", password.clone()));
        }
        params
    }

    /// True if developer credentials are missing — caller should return
    /// `NotConfigured` without opening a socket.
    fn is_configured(&self) -> bool {
        !self.dev_id.is_empty() && !self.dev_password.is_empty()
    }

    /// Issue a GET to `{base_url}/{endpoint}` with the given extra params,
    /// map HTTP-level failures to `ProviderError`, and return the parsed JSON
    /// body. A 429 maps to `RateLimited`; any other non-2xx maps to `Network`.
    fn get_json(
        &self,
        endpoint: &str,
        extra: &[(&'static str, String)],
    ) -> Result<serde_json::Value, ProviderError> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut params = self.auth_params();
        for (k, v) in extra {
            params.push((*k, v.clone()));
        }
        let resp = self
            .http
            .get(&url)
            .query(&params)
            .send()
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            // VERIFY: ScreenScraper's non-2xx error body shape (likely a JSON
            // object with a message field) is not parsed here; the status text
            // is enough for the `Network` error message.
            return Err(ProviderError::Network(format!("HTTP {}", status)));
        }
        let body = resp
            .text()
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        serde_json::from_str::<serde_json::Value>(&body)
            .map_err(|e| ProviderError::Network(format!("invalid JSON: {e}")))
    }
}

impl Provider for ScreenScraperProvider {
    fn id(&self) -> &'static str {
        "screenscraper"
    }

    fn search_by_hash(&self, query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError> {
        if !self.is_configured() {
            return Err(ProviderError::NotConfigured);
        }

        // VERIFY: ScreenScraper's `jeuInfos.php` accepts exactly one of
        // `crc`, `md5`, or `sha1` for hash lookup. We prefer md5 (full-file)
        // then crc32 then sha1; confirm param names + preferred hash order
        // against current docs. CRC32 is sent as an 8-char hex string.
        let mut extra: Vec<(&'static str, String)> = Vec::new();
        if let Some(md5) = query.md5 {
            extra.push(("md5", md5.to_string()));
        } else if let Some(crc) = query.crc32 {
            extra.push(("crc", format!("{:08x}", crc)));
        } else {
            // No hash available — hash lookup cannot proceed. Mirror the
            // trait contract: providers that can't support hash lookup return
            // an empty result rather than erroring, so the pipeline falls
            // through to filename search uniformly.
            return Ok(vec![]);
        }

        // VERIFY: ScreenScraper filters by numeric `systemeid` or string
        // `systemname`; passing our slug as `systemname` may not match its
        // naming scheme (e.g. we use "snes", ScreenScraper may expect "snes"
        // or "super-nintendo"). Left in to scope the query; treat a mismatch
        // as "no hit" rather than an error.
        extra.push(("systemname", query.system_slug.to_string()));

        let json = self.get_json("jeuInfos.php", &extra)?;

        // VERIFY: a hash miss is signalled by `response.jeu` being null/absent
        // (versus an HTTP error or a JSON error field). Confirm against docs;
        // if ScreenScraper instead returns a top-level `response.error`, this
        // branch should detect that too.
        let jeu = json.get("response").and_then(|r| r.get("jeu"));
        match jeu {
            None => Ok(vec![]),
            Some(v) if v.is_null() => Ok(vec![]),
            Some(jeu) => {
                let Some(c) = candidate_from_jeu(jeu, query.system_slug) else {
                    return Ok(vec![]);
                };
                Ok(vec![c])
            }
        }
    }

    fn search_by_name(&self, query: &SearchQuery) -> Result<Vec<Candidate>, ProviderError> {
        if !self.is_configured() {
            return Err(ProviderError::NotConfigured);
        }

        // VERIFY: the search-by-name endpoint is `jeuRecherche.php` and takes
        // a `recherche` (search term) param. Confirm endpoint + param name
        // against current ScreenScraper API v2 docs. The search term used is
        // the file stem (extension stripped) since the raw filename rarely
        // matches a title.
        let stem = std::path::Path::new(query.filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(query.filename);
        let extra: Vec<(&'static str, String)> = vec![
            ("recherche", stem.to_string()),
            ("systemname", query.system_slug.to_string()),
        ];

        let json = self.get_json("jeuRecherche.php", &extra)?;

        // VERIFY: the search response shape is `response.jeux` (an array),
        // each element the same `jeu` object shape `jeuInfos.php` returns for
        // a single game. Confirm against docs.
        let jeux = json
            .get("response")
            .and_then(|r| r.get("jeux"))
            .and_then(|j| j.as_array());
        let Some(jeux) = jeux else {
            // No `jeux` array — treat as no matches rather than an error, so
            // the pipeline can fall through to other providers.
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        for jeu in jeux {
            if let Some(c) = candidate_from_jeu(jeu, query.system_slug) {
                out.push(c);
            }
        }
        // Don't rank here — `pipeline.rs` does confidence scoring on top of
        // whatever we return (PLAN.md §7.1).
        Ok(out)
    }
}

/// Map a ScreenScraper `jeu` JSON object to a `Candidate`. Returns `None` if
/// the object is missing an `id` or any usable name (treated as "skip this
/// entry" rather than an error, so a single malformed entry can't poison the
/// whole result list).
fn candidate_from_jeu(jeu: &serde_json::Value, system_slug: &str) -> Option<Candidate> {
    // VERIFY: `jeu.id` is a stringified integer in ScreenScraper responses;
    // we take it verbatim as `external_id`.
    let external_id = jeu.get("id").and_then(|v| v.as_str())?.to_string();

    // VERIFY: `jeu.noms` is an array of regional names, each shaped like
    // `{"region": "us", "text": "Super Metroid"}`. We take the first
    // available `text` — a real implementation should prefer the user's
    // region, but ranking is the pipeline's job, not the provider's.
    let name = jeu
        .get("noms")
        .and_then(|n| n.as_array())
        .and_then(|arr| {
            arr.iter()
                .find_map(|n| n.get("text").and_then(|t| t.as_str()))
        })
        .map(str::to_string)?;
    Some(Candidate {
        external_id,
        name,
        system_slug: system_slug.to_string(),
    })
}

#[cfg(test)]
mod tests {
    //! No test in this module performs real network access. Every test either
    //! points `ScreenScraperProvider` at a local `TcpListener`-based mock
    //! server bound to 127.0.0.1, or makes no HTTP request at all (the
    //! `NotConfigured` test). This matches the project convention (see
    //! `core/src/scan` and `modules/themes` test suites): zero live network
    //! calls in tests, ever.

    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Spawn a one-shot mock HTTP server on 127.0.0.1 returning `status_line`
    /// (e.g. `"200 OK"`) and `body`. Returns the port it bound. The server
    /// accepts exactly one connection, drains the request headers, writes the
    /// canned response, and closes — enough for a single reqwest blocking call.
    fn mock_server(status_line: &str, body: &str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = listener.local_addr().expect("local_addr").port();
        let status_line = status_line.to_string();
        let body = body.to_string();
        std::thread::spawn(move || {
            let (mut sock, _) = match listener.accept() {
                Ok(p) => p,
                Err(_) => return,
            };
            // Drain request headers (up to end-of-headers) so the client is
            // happy; we don't care about the request contents.
            let mut buf = [0u8; 4096];
            let mut got = String::new();
            while !got.contains("\r\n\r\n") {
                match sock.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => got.push_str(&String::from_utf8_lossy(&buf[..n])),
                    Err(_) => break,
                }
            }
            let resp = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes());
            let _ = sock.flush();
        });
        port
    }

    fn provider_at(port: u16) -> ScreenScraperProvider {
        ScreenScraperProvider::new("dev", "secret")
            .with_base_url(format!("http://127.0.0.1:{port}"))
    }

    fn query<'a>(
        system_slug: &'a str,
        filename: &'a str,
        crc32: Option<u32>,
        md5: Option<&'a str>,
    ) -> SearchQuery<'a> {
        SearchQuery {
            system_slug,
            filename,
            crc32,
            md5,
        }
    }

    #[test]
    fn not_configured_when_dev_creds_empty() {
        // No socket is opened: `is_configured` short-circuits first.
        let provider = ScreenScraperProvider::new("", "");
        let q = query("snes", "Super Metroid.sfc", Some(0xDEADBEEF), None);
        match provider.search_by_hash(&q) {
            Err(ProviderError::NotConfigured) => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }
        match provider.search_by_name(&q) {
            Err(ProviderError::NotConfigured) => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn hash_hit_parses_to_single_candidate() {
        // VERIFY-shaped body: `response.jeu.id` + `response.jeu.noms[].text`.
        let body = r#"{
            "response": {
                "jeu": {
                    "id": "123",
                    "noms": [
                        {"region": "us", "text": "Super Metroid"},
                        {"region": "jp", "text": "Super Metroid (Japan)"}
                    ]
                }
            }
        }"#;
        let port = mock_server("200 OK", body);
        let provider = provider_at(port);
        let q = query("snes", "Super Metroid.sfc", Some(0xDEADBEEF), None);
        let results = provider.search_by_hash(&q).expect("hash search ok");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].external_id, "123");
        assert_eq!(results[0].name, "Super Metroid");
        assert_eq!(results[0].system_slug, "snes");
    }

    #[test]
    fn hash_miss_returns_empty_vec_not_error() {
        // VERIFY: a not-found is signalled by `response.jeu` being null.
        let body = r#"{"response":{"jeu":null}}"#;
        let port = mock_server("200 OK", body);
        let provider = provider_at(port);
        let q = query("snes", "Unknown.sfc", Some(0x12345678), None);
        let results = provider.search_by_hash(&q).expect("miss should not error");
        assert!(results.is_empty());
    }

    #[test]
    fn http_429_maps_to_rate_limited() {
        let port = mock_server("429 Too Many Requests", "{}");
        let provider = provider_at(port);
        let q = query("snes", "Super Metroid.sfc", Some(0xDEADBEEF), None);
        match provider.search_by_hash(&q) {
            Err(ProviderError::RateLimited) => {}
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn name_search_returns_every_candidate() {
        // VERIFY: search response is `response.jeux` (array of jeu objects).
        let body = r#"{
            "response": {
                "jeux": [
                    {"id":"11","noms":[{"region":"us","text":"Super Metroid"}]},
                    {"id":"12","noms":[{"region":"us","text":"Metroid"}]},
                    {"id":"13","noms":[{"region":"us","text":"Metroid Prime"}]}
                ]
            }
        }"#;
        let port = mock_server("200 OK", body);
        let provider = provider_at(port);
        let q = query("snes", "Metroid.sfc", None, None);
        let results = provider.search_by_name(&q).expect("name search ok");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "Super Metroid");
        assert_eq!(results[1].name, "Metroid");
        assert_eq!(results[2].name, "Metroid Prime");
        // system_slug is passed through unchanged.
        assert!(results.iter().all(|c| c.system_slug == "snes"));
    }

    #[test]
    fn name_search_429_maps_to_rate_limited() {
        let port = mock_server("429 Too Many Requests", "{}");
        let provider = provider_at(port);
        let q = query("snes", "Metroid.sfc", None, None);
        match provider.search_by_name(&q) {
            Err(ProviderError::RateLimited) => {}
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn with_user_attaches_user_credentials() {
        // No socket opened: we only inspect that auth_params reflects the user.
        let provider = ScreenScraperProvider::new("dev", "secret").with_user("alice", "hunter2");
        let params = provider.auth_params();
        let has = |k: &str| params.iter().any(|(pk, _)| *pk == k);
        assert!(has("devid"));
        assert!(has("devpassword"));
        assert!(has("softname"));
        assert!(has("ssid"));
        assert!(has("sspassword"));
    }
}
