use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::Paper;

const FIELDS: &str = "title,abstract,year,citationCount,authors,url,publicationDate,externalIds,fieldsOfStudy,openAccessPdf,venue";

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

const MIN_INTERVAL_AUTH: Duration = Duration::from_millis(1000);
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
static WARNED: OnceLock<()> = OnceLock::new();

fn http_get_with_retry_cfg(
    url: &str,
    api_key: Option<String>,
    _cfg: &BackoffConfig,
) -> FetchOutcome {
    // Task 1 占位：保持原有简单行为，后续任务渐进替换。
    // - 切到 Agent + http_status_as_error(false)
    // - 加 User-Agent
    // - 429 / 5xx / 其它错误一律按 Err 返回（保留旧行为）
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();

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
        200 => match resp.into_body().read_to_string() {
            Ok(body) => FetchOutcome::Ok(body),
            Err(e) => FetchOutcome::Err(format!("read body: {e}")),
        },
        404 => FetchOutcome::Err("HTTP 404".to_string()),
        429 => FetchOutcome::Err("rate limited (429)".to_string()),
        500..=599 => FetchOutcome::Err(format!("Server error: {status}")),
        _ => FetchOutcome::Err(format!("HTTP {status}")),
    }
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
        FetchOutcome::RateLimited => Err("rate limited (429)".to_string()),
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
        FetchOutcome::RateLimited => Err("rate limited (429)".to_string()),
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
    #[serial]
    fn request_sends_user_agent_header() {
        unsafe { std::env::remove_var("SEMANTIC_SCHOLAR_API_KEY") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header(
                "user-agent",
                mockito::Matcher::Regex("fastpaper-cli/".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }
}
