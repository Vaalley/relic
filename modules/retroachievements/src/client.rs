//! RetroAchievements Connect API and REST API client groundwork (design doc §3).
//!
//! Handles hash lookup (T1 anonymous mode or T2 authenticated mode) and achievement/user
//! progress fetches. This client implements rate limits, exponential backoff + jitter,
//! and a circuit breaker to avoid spamming the RetroAchievements servers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

/// Match structure containing the resolved game ID from RetroAchievements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameIdMatch {
    pub game_id: i64,
}

/// Errors returned by the RetroAchievements client.
#[derive(Debug)]
pub enum RaClientError {
    /// Network-level connection or response reading issues.
    Network(String),
    /// Rate-limit (429) hit.
    RateLimited,
    /// Circuit breaker is open, preventing further requests.
    CircuitBreakerOpen,
    /// Non-success HTTP status code returned.
    HttpError(u16, String),
}

impl std::fmt::Display for RaClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {}", msg),
            Self::RateLimited => write!(f, "rate limited (429)"),
            Self::CircuitBreakerOpen => write!(f, "circuit breaker is open"),
            Self::HttpError(code, msg) => write!(f, "HTTP error {}: {}", code, msg),
        }
    }
}

impl std::error::Error for RaClientError {}

/// Configuration for client rate-limiting, retrying, and circuit breaker behavior.
#[derive(Debug, Clone)]
pub struct RaClientConfig {
    /// Maximum allowed requests per second.
    pub max_reqs_per_sec: f64,
    /// Maximum allowed requests per minute.
    pub max_reqs_per_min: usize,
    /// Initial duration to wait when backing off after a 429/5xx.
    pub base_backoff: Duration,
    /// Maximum duration allowed for backoff.
    pub max_backoff: Duration,
    /// Maximum number of retries per request before failing.
    pub max_retries: usize,
    /// Number of consecutive failures before the circuit breaker trips.
    pub circuit_breaker_failures_threshold: usize,
    /// Time the circuit breaker remains open before resetting.
    pub circuit_breaker_duration: Duration,
}

impl Default for RaClientConfig {
    fn default() -> Self {
        Self {
            max_reqs_per_sec: 5.0,
            max_reqs_per_min: 200,
            base_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(60),
            max_retries: 3,
            circuit_breaker_failures_threshold: 5,
            circuit_breaker_duration: Duration::from_secs(600), // 10 minutes
        }
    }
}

/// Internal rate limiter and circuit breaker state.
struct RateLimiterState {
    recent_requests: Vec<Instant>,
    consecutive_failures: usize,
    circuit_breaker_tripped_at: Option<Instant>,
}

/// HTTP client for interacting with the RetroAchievements Connect and REST APIs.
pub struct RaHashClient {
    base_url: String,
    username: Option<String>,
    api_key: Option<String>,
    config: RaClientConfig,
    http: reqwest::blocking::Client,
    state: Mutex<RateLimiterState>,
    /// Number of times a 429 Rate Limited response was observed (for testing).
    pub(crate) rate_limit_hit_count: AtomicUsize,
}

#[derive(Deserialize)]
struct GameIdResponse {
    #[serde(rename = "GameID")]
    game_id: Option<i64>,
}

impl Default for RaHashClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RaHashClient {
    /// Creates a new RetroAchievements client in anonymous T1 mode.
    pub fn new() -> Self {
        Self {
            base_url: "https://retroachievements.org".to_string(),
            username: None,
            api_key: None,
            config: RaClientConfig::default(),
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build reqwest blocking client"),
            state: Mutex::new(RateLimiterState {
                recent_requests: Vec::new(),
                consecutive_failures: 0,
                circuit_breaker_tripped_at: None,
            }),
            rate_limit_hit_count: AtomicUsize::new(0),
        }
    }

    /// Builder: attaches a user credentials pair (u & y params) for authorized requests.
    pub fn with_credentials(
        mut self,
        username: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.api_key = Some(api_key.into());
        self
    }

    /// Builder: overrides the base API URL (primarily for mock server testing).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Builder: provides custom rate limit/backoff configuration (for tests).
    pub fn with_config(mut self, config: RaClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Look up a game's ID by its content MD5 hash.
    ///
    /// The API endpoint used is `dorequest.php` with `r=gameid` on the legacy Connect API.
    /// If `GameID` in the response is 0 or absent, it is treated as a hash miss (`Ok(None)`).
    ///
    /// VERIFY: console_id/title need a follow-up API_GetGame.php call, not covered by this client yet.
    pub fn lookup_hash(&self, md5_hex: &str) -> Result<Option<GameIdMatch>, RaClientError> {
        let url = format!("{}/dorequest.php", self.base_url);

        let mut query_params = vec![("r", "gameid"), ("m", md5_hex)];

        // Attach credentials if present, but allow anonymous T1 requests.
        // VERIFY: some RA client libraries attach a username+API key pair
        // (params named u and y) even to unauthenticated-feeling calls.
        if let Some((user, key)) = self.username.as_deref().zip(self.api_key.as_deref()) {
            query_params.push(("u", user));
            query_params.push(("y", key));
        }

        let resp = self.execute_request_with_backoff(&url, &query_params)?;

        let text = resp
            .text()
            .map_err(|e| RaClientError::Network(e.to_string()))?;
        let parsed: GameIdResponse = serde_json::from_str(&text)
            .map_err(|e| RaClientError::Network(format!("failed to parse JSON: {}", e)))?;

        match parsed.game_id {
            Some(0) | None => Ok(None),
            Some(id) => Ok(Some(GameIdMatch { game_id: id })),
        }
    }

    /// Fetch achievement metadata and user progress.
    ///
    /// This is a stub for sub-phase 6b/6c progress integration.
    /// Uses modern v1 REST API at `API_GetGameInfoAndUserProgress.php`.
    pub fn get_game_info_and_user_progress(
        &self,
        game_id: i64,
    ) -> Result<serde_json::Value, RaClientError> {
        let url = format!("{}/API/API_GetGameInfoAndUserProgress.php", self.base_url);

        let user = self.username.as_deref().unwrap_or("");
        let key = self.api_key.as_deref().unwrap_or("");
        let game_id_str = game_id.to_string();

        let query_params = vec![("u", user), ("y", key), ("g", &game_id_str)];

        let resp = self.execute_request_with_backoff(&url, &query_params)?;
        let text = resp
            .text()
            .map_err(|e| RaClientError::Network(e.to_string()))?;
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| RaClientError::Network(format!("failed to parse JSON: {}", e)))?;

        Ok(parsed)
    }

    fn pre_request_check_and_wait(&self) -> Result<(), RaClientError> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();

        // 1. Check circuit breaker status
        if let Some(tripped_at) = state.circuit_breaker_tripped_at {
            if now.duration_since(tripped_at) < self.config.circuit_breaker_duration {
                return Err(RaClientError::CircuitBreakerOpen);
            } else {
                state.circuit_breaker_tripped_at = None;
                state.consecutive_failures = 0;
            }
        }

        // 2. Clean up old requests (> 60 seconds)
        state
            .recent_requests
            .retain(|&t| now.duration_since(t) < Duration::from_secs(60));

        // 3. Enforce requests/second limit
        let min_sec_delay = Duration::from_secs_f64(1.0 / self.config.max_reqs_per_sec);
        if let Some(&last_req) = state.recent_requests.last() {
            let elapsed = now.duration_since(last_req);
            if elapsed < min_sec_delay {
                let sleep_dur = min_sec_delay - elapsed;
                std::thread::sleep(sleep_dur);
            }
        }

        // 4. Enforce requests/minute limit
        let now = Instant::now();
        if state.recent_requests.len() >= self.config.max_reqs_per_min {
            let oldest_req = state.recent_requests[0];
            let elapsed = now.duration_since(oldest_req);
            let one_min = Duration::from_secs(60);
            if elapsed < one_min {
                let sleep_dur = one_min - elapsed;
                std::thread::sleep(sleep_dur);
            }
        }

        state.recent_requests.push(Instant::now());
        Ok(())
    }

    fn execute_request_with_backoff(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<reqwest::blocking::Response, RaClientError> {
        let mut attempt = 0;

        loop {
            self.pre_request_check_and_wait()?;

            let mut req = self.http.get(url);
            if !params.is_empty() {
                req = req.query(params);
            }

            let res = req
                .send()
                .map_err(|e| RaClientError::Network(e.to_string()));

            let resp = match res {
                Ok(r) => r,
                Err(e) => {
                    self.record_failure();
                    if attempt < self.config.max_retries {
                        attempt += 1;
                        let backoff = self.calculate_backoff(attempt);
                        std::thread::sleep(backoff);
                        continue;
                    }
                    return Err(e);
                }
            };

            let status = resp.status();
            if status.is_success() {
                self.record_success();
                return Ok(resp);
            }

            if status.as_u16() == 429 || status.is_server_error() {
                if status.as_u16() == 429 {
                    self.rate_limit_hit_count.fetch_add(1, Ordering::SeqCst);
                }

                self.record_failure();

                if attempt < self.config.max_retries {
                    attempt += 1;
                    let backoff = self.calculate_backoff(attempt);
                    std::thread::sleep(backoff);
                    continue;
                }

                if status.as_u16() == 429 {
                    return Err(RaClientError::RateLimited);
                } else {
                    return Err(RaClientError::HttpError(
                        status.as_u16(),
                        format!("HTTP status {}", status),
                    ));
                }
            } else {
                self.record_failure();
                return Err(RaClientError::HttpError(
                    status.as_u16(),
                    format!("HTTP status {}", status),
                ));
            }
        }
    }

    fn record_success(&self) {
        let mut state = self.state.lock().unwrap();
        state.consecutive_failures = 0;
        state.circuit_breaker_tripped_at = None;
    }

    fn record_failure(&self) {
        let mut state = self.state.lock().unwrap();
        state.consecutive_failures += 1;
        if state.consecutive_failures >= self.config.circuit_breaker_failures_threshold {
            state.circuit_breaker_tripped_at = Some(Instant::now());
        }
    }

    fn calculate_backoff(&self, attempt: usize) -> Duration {
        let multiplier = 1u32.checked_shl(attempt as u32 - 1).unwrap_or(u32::MAX) as u64;
        let base_ms = self.config.base_backoff.as_millis() as u64;
        let backoff_ms = base_ms.saturating_mul(multiplier);

        let max_jitter_ms = (backoff_ms / 5).max(50);
        let jitter_ms = get_jitter_ms(max_jitter_ms);

        let total_ms = backoff_ms.saturating_add(jitter_ms);
        let total_dur = Duration::from_millis(total_ms);

        if total_dur > self.config.max_backoff {
            self.config.max_backoff
        } else {
            total_dur
        }
    }
}

fn get_jitter_ms(max_jitter_ms: u64) -> u64 {
    let nano = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(42);
    let seed = nano.wrapping_mul(6364136223846793005).wrapping_add(1);
    seed % (max_jitter_ms + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn multi_response_mock_server(responses: Vec<(String, String)>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            for (status_line, body) in responses {
                let (mut sock, _) = match listener.accept() {
                    Ok(p) => p,
                    Err(_) => break,
                };
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
            }
        });
        port
    }

    #[test]
    fn test_rate_limiting_retry_and_success() {
        let responses = vec![
            ("429 Too Many Requests".to_string(), "{}".to_string()),
            ("200 OK".to_string(), r#"{"GameID": 12345}"#.to_string()),
        ];
        let port = multi_response_mock_server(responses);

        let config = RaClientConfig {
            base_backoff: Duration::from_millis(5),
            max_backoff: Duration::from_millis(50),
            max_retries: 3,
            ..Default::default()
        };

        let client = RaHashClient::new()
            .with_base_url(format!("http://127.0.0.1:{}", port))
            .with_config(config);

        let result = client
            .lookup_hash("abcdef1234567890")
            .expect("should eventually succeed");
        assert_eq!(result, Some(GameIdMatch { game_id: 12345 }));
        assert_eq!(client.rate_limit_hit_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_hash_miss_gameid_zero() {
        let responses = vec![("200 OK".to_string(), r#"{"GameID": 0}"#.to_string())];
        let port = multi_response_mock_server(responses);
        let client = RaHashClient::new().with_base_url(format!("http://127.0.0.1:{}", port));

        let result = client
            .lookup_hash("abcdef1234567890")
            .expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_hash_miss_gameid_absent() {
        let responses = vec![("200 OK".to_string(), r#"{}"#.to_string())];
        let port = multi_response_mock_server(responses);
        let client = RaHashClient::new().with_base_url(format!("http://127.0.0.1:{}", port));

        let result = client
            .lookup_hash("abcdef1234567890")
            .expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_circuit_breaker_trips() {
        let responses = vec![
            ("500 Internal Server Error".to_string(), "{}".to_string()),
            ("500 Internal Server Error".to_string(), "{}".to_string()),
            ("500 Internal Server Error".to_string(), "{}".to_string()),
        ];
        let port = multi_response_mock_server(responses);

        let config = RaClientConfig {
            base_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            max_retries: 2,
            circuit_breaker_failures_threshold: 2,
            circuit_breaker_duration: Duration::from_secs(10),
            ..Default::default()
        };

        let client = RaHashClient::new()
            .with_base_url(format!("http://127.0.0.1:{}", port))
            .with_config(config);

        let result = client.lookup_hash("abcdef1234567890");
        assert!(result.is_err());

        let next_result = client.lookup_hash("abcdef1234567890");
        match next_result {
            Err(RaClientError::CircuitBreakerOpen) => {}
            other => panic!("expected CircuitBreakerOpen, got {:?}", other),
        }
    }
}
