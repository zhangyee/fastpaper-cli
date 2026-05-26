use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::Paper;

const FIELDS: &str = "title,abstract,year,citationCount,authors,url,publicationDate,externalIds,fieldsOfStudy,openAccessPdf,venue";

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

// 节流间隔（带 key）。
const MIN_INTERVAL_AUTH: Duration = Duration::from_millis(1000);
// 节流间隔（匿名）。
const MIN_INTERVAL_ANON: Duration = Duration::from_millis(100);
const MAX_RETRIES: u32 = 5;

#[derive(Clone, Copy)]
struct BackoffConfig {
    base: Duration,
    max: Duration,
    max_retries: u32,
}

impl BackoffConfig {
    const DEFAULT_AUTH: Self = Self {
        base: Duration::from_secs(1),
        max: Duration::from_secs(30),
        max_retries: MAX_RETRIES,
    };
    const DEFAULT_ANON: Self = Self {
        base: Duration::from_secs(2),
        max: Duration::from_secs(30),
        max_retries: MAX_RETRIES,
    };
}

enum FetchOutcome {
    Ok(String),
    RateLimited,
    Err(String),
}

static LAST_CALL: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
// Task 7 search 限流降级路径使用。
#[allow(dead_code)]
static WARNED: OnceLock<()> = OnceLock::new();

// 解析 HTTP Retry-After 头。
fn parse_retry_after(headers: &ureq::http::HeaderMap, max: Duration) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .map(|d| d.min(max))
}

// 指数退避（带 0-10% jitter）。
fn backoff_delay(attempt: u32, cfg: &BackoffConfig) -> Duration {
    let shift = attempt.min(10);
    let exp_ms = (cfg.base.as_millis() as u64).saturating_mul(1u64 << shift);
    let exp = Duration::from_millis(exp_ms).min(cfg.max);
    // 轻量 jitter：用 Instant nanos 做伪随机 0..=10%
    let nanos = Instant::now().elapsed().subsec_nanos() as u64;
    let jitter_pct = nanos % 11; // 0..=10
    let jitter_ms = (exp.as_millis() as u64).saturating_mul(jitter_pct) / 100;
    exp + Duration::from_millis(jitter_ms)
}

// 进程级节流。第二次连续调用会 sleep 到至少 min_interval。
fn throttle_with(state: &Mutex<Option<Instant>>, min_interval: Duration) {
    let mut guard = state.lock().unwrap();
    if let Some(last) = *guard {
        let elapsed = last.elapsed();
        if elapsed < min_interval {
            std::thread::sleep(min_interval - elapsed);
        }
    }
    *guard = Some(Instant::now());
}

fn throttle(has_key: bool) {
    let min = if has_key { MIN_INTERVAL_AUTH } else { MIN_INTERVAL_ANON };
    let state = LAST_CALL.get_or_init(|| Mutex::new(None));
    throttle_with(state, min);
}

fn http_get_with_retry_cfg(
    url: &str,
    api_key: Option<String>,
    cfg: &BackoffConfig,
) -> FetchOutcome {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();

    for attempt in 0..=cfg.max_retries {
        throttle(api_key.is_some());

        let mut req = agent.get(url).header("User-Agent", USER_AGENT);
        if let Some(ref k) = api_key {
            req = req.header("x-api-key", k);
        }
        let resp = match req.call() {
            Ok(r) => r,
            Err(e) => return FetchOutcome::Err(format!("HTTP error: {e}")),
        };
        let status = resp.status().as_u16();
        match status {
            200 => {
                return match resp.into_body().read_to_string() {
                    Ok(body) => FetchOutcome::Ok(body),
                    Err(e) => FetchOutcome::Err(format!("read body: {e}")),
                };
            }
            404 => return FetchOutcome::Err("HTTP 404".to_string()),
            429 => {
                if attempt == cfg.max_retries {
                    return FetchOutcome::RateLimited;
                }
                let wait = parse_retry_after(resp.headers(), cfg.max)
                    .unwrap_or_else(|| backoff_delay(attempt, cfg));
                std::thread::sleep(wait);
            }
            500..=599 => return FetchOutcome::Err(format!("Server error: {status}")),
            _ => return FetchOutcome::Err(format!("HTTP {status}")),
        }
    }
    FetchOutcome::RateLimited
}

/// Search Semantic Scholar API and return parsed papers.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let encoded = super::encode_query(query);
    let url = format!(
        "{}/graph/v1/paper/search?query={}&limit={}&fields={}",
        base_url, encoded, max_results, FIELDS
    );
    let api_key = std::env::var("SEMANTIC_SCHOLAR_API_KEY").ok();
    let cfg = if api_key.is_some() {
        BackoffConfig::DEFAULT_AUTH
    } else {
        BackoffConfig::DEFAULT_ANON
    };
    match http_get_with_retry_cfg(&url, api_key, &cfg) {
        FetchOutcome::Ok(body) => parse_search_response(&body),
        FetchOutcome::RateLimited => Err(format!("rate limited after {} retries", cfg.max_retries)),
        FetchOutcome::Err(e) => Err(e),
    }
}

/// Fetch a single paper by S2 paper ID.
pub fn get_by_id(base_url: &str, s2_id: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/graph/v1/paper/{}?fields={}",
        base_url, s2_id, FIELDS
    );
    let api_key = std::env::var("SEMANTIC_SCHOLAR_API_KEY").ok();
    let cfg = if api_key.is_some() {
        BackoffConfig::DEFAULT_AUTH
    } else {
        BackoffConfig::DEFAULT_ANON
    };
    match http_get_with_retry_cfg(&url, api_key, &cfg) {
        FetchOutcome::Ok(body) => {
            let wrapped = format!(r#"{{"data":[{}]}}"#, body);
            Ok(parse_search_response(&wrapped)?.into_iter().next())
        }
        FetchOutcome::Err(e) if e.contains("404") => Ok(None),
        FetchOutcome::Err(e) => Err(e),
        FetchOutcome::RateLimited => Err(format!(
            "rate limited after {} retries",
            cfg.max_retries
        )),
    }
}

/// Parse Semantic Scholar JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let data = root["data"]
        .as_array()
        .ok_or("missing 'data' array")?;

    let mut papers = Vec::new();
    for item in data {
        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["externalIds"]["DOI"].as_str().map(|s| s.to_string());

        let pdf_url = item["openAccessPdf"]["url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.contains("arxiv.org/abs/") {
                    s.replace("/abs/", "/pdf/")
                } else {
                    s.to_string()
                }
            });

        let fields: Vec<String> = item["fieldsOfStudy"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let citations = item["citationCount"].as_u64().map(|n| n as u32);

        papers.push(Paper {
            id: item["paperId"].as_str().unwrap_or("").to_string(),
            title: item["title"].as_str().unwrap_or("").to_string(),
            authors,
            abstract_text: item["abstract"].as_str().map(|s| s.to_string()),
            year: item["year"].as_u64().map(|y| y as u16),
            doi,
            url: item["url"].as_str().map(|s| s.to_string()),
            pdf_url,
            venue: item["venue"].as_str().map(|s| s.to_string()),
            citations,
            fields,
            open_access: Some(item["openAccessPdf"].is_object() && !item["openAccessPdf"].is_null()),
            source: "semantic".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const FIXTURE: &str = include_str!("../../tests/fixtures/semantic_search.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_papers_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers.is_empty());
    }

    #[test]
    fn parse_titles_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_source_is_semantic() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "semantic");
        }
    }

    #[test]
    fn parse_citations_present() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
            assert!(p.citations.unwrap() > 0);
        }
    }

    #[test]
    fn parse_pdf_url_from_open_access() {
        // Our fixture has openAccessPdf as null for all papers
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.pdf_url.is_none(), "paper {} should have no pdf_url", p.id);
        }
    }

    #[test]
    fn parse_doi_from_external_ids() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // First paper has DOI in externalIds
        let first = &papers[0];
        assert_eq!(
            first.doi.as_deref(),
            Some("10.1016/J.NEUCOM.2021.03.091")
        );
    }

    #[test]
    fn parse_empty_data_returns_empty_list() {
        let papers = parse_search_response(r#"{"data": []}"#).unwrap();
        assert!(papers.is_empty());
    }

    #[test]
    fn search_returns_papers() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(&server.url(), "test", 3).unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_request_path_contains_paper_search() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("paper/search".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_query_param() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("query=test".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_limit() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("limit=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    #[serial]
    fn search_sends_api_key_header_when_set() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("x-api-key", "test-key-123")
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        unsafe { std::env::set_var("SEMANTIC_SCHOLAR_API_KEY", "test-key-123") };
        let _ = search(&server.url(), "test", 3);
        unsafe { std::env::remove_var("SEMANTIC_SCHOLAR_API_KEY") };
        mock.assert();
    }

    #[test]
    #[serial]
    fn search_works_without_api_key() {
        unsafe { std::env::remove_var("SEMANTIC_SCHOLAR_API_KEY") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(&server.url(), "test", 3);
        assert!(result.is_ok());
        mock.assert();
    }

    #[test]
    fn parse_retry_after_seconds() {
        let cap = Duration::from_secs(30);

        // helper 构造一个只含 retry-after 的 HeaderMap
        fn headers_with_retry_after(value: &str) -> ureq::http::HeaderMap {
            let mut h = ureq::http::HeaderMap::new();
            h.insert(
                ureq::http::header::HeaderName::from_static("retry-after"),
                ureq::http::HeaderValue::from_str(value).unwrap(),
            );
            h
        }

        assert_eq!(
            parse_retry_after(&headers_with_retry_after("5"), cap),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("abc"), cap),
            None
        );
        assert_eq!(
            parse_retry_after(&ureq::http::HeaderMap::new(), cap),
            None
        );
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("9999"), cap),
            Some(cap)
        );
    }

    #[test]
    fn backoff_delay_grows_exponentially() {
        let cfg = BackoffConfig {
            base: Duration::from_millis(100),
            max: Duration::from_secs(5),
            max_retries: 5,
        };
        let d0 = backoff_delay(0, &cfg);
        let d2 = backoff_delay(2, &cfg);
        assert!(d0 < d2, "delay should grow with attempt");
        assert!(d2 <= cfg.max + cfg.max / 10, "delay should respect max (with up-to-10% jitter)");
    }

    #[test]
    fn backoff_delay_does_not_overflow() {
        let cfg = BackoffConfig {
            base: Duration::from_secs(1),
            max: Duration::from_secs(30),
            max_retries: 5,
        };
        // attempt = 20 不应导致左移溢出
        let d = backoff_delay(20, &cfg);
        assert!(d <= cfg.max + cfg.max / 10);
    }

    #[test]
    fn throttle_with_sleeps_when_called_back_to_back() {
        let state: Mutex<Option<Instant>> = Mutex::new(None);
        let interval = Duration::from_millis(80);

        let t0 = Instant::now();
        throttle_with(&state, interval);
        throttle_with(&state, interval);
        let elapsed = t0.elapsed();

        assert!(
            elapsed >= interval,
            "second call should wait at least one interval (got {:?})",
            elapsed
        );
    }

    #[test]
    fn throttle_with_does_not_sleep_after_long_gap() {
        let past = Instant::now() - Duration::from_secs(3600);
        let state: Mutex<Option<Instant>> = Mutex::new(Some(past));
        let interval = Duration::from_millis(100);

        let t0 = Instant::now();
        throttle_with(&state, interval);
        let elapsed = t0.elapsed();

        assert!(
            elapsed < Duration::from_millis(20),
            "should return immediately after long gap (got {:?})",
            elapsed
        );
    }

    #[test]
    #[serial]
    fn request_sends_user_agent_header() {
        unsafe { std::env::remove_var("SEMANTIC_SCHOLAR_API_KEY") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header(
                "user-agent",
                mockito::Matcher::Regex(
                    r"^fastpaper-cli/\d+\.\d+\.\d+ \(\+https://".to_string(),
                ),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    #[serial]
    fn rate_limit_then_success_respects_retry_after() {
        unsafe { std::env::remove_var("SEMANTIC_SCHOLAR_API_KEY") };
        let mut server = mockito::Server::new();

        // 第 1 次：429 + Retry-After: 1
        let m1 = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .with_header("retry-after", "1")
            .expect(1)
            .create();
        // 第 2 次：200
        let m2 = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();

        let t0 = Instant::now();
        let result = search(&server.url(), "test", 3);
        let elapsed = t0.elapsed();

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert!(!result.unwrap().is_empty());
        assert!(
            elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(3),
            "expected ~1s wait (got {:?})",
            elapsed
        );
        m1.assert();
        m2.assert();
    }
}
