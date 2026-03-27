use super::Paper;

const FIELDS: &str = "title,abstract,year,citationCount,authors,url,publicationDate,externalIds,fieldsOfStudy,openAccessPdf,venue";

/// Search Semantic Scholar API and return parsed papers.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let url = format!(
        "{}/graph/v1/paper/search?query={}&limit={}&fields={}",
        base_url, query, max_results, FIELDS
    );

    let api_key = std::env::var("SEMANTIC_SCHOLAR_API_KEY").ok();

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        let mut req = ureq::get(&url);
        if let Some(ref key) = api_key {
            req = req.header("x-api-key", key);
        }
        match req.call() {
            Ok(resp) => {
                let body = resp
                    .into_body()
                    .read_to_string()
                    .map_err(|e| format!("Failed to read response: {}", e))?;
                return parse_search_response(&body);
            }
            Err(ureq::Error::StatusCode(429)) => {
                last_err = "rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("Server error: {}", code));
            }
            Err(e) => {
                return Err(format!("HTTP error: {}", e));
            }
        }
    }
    Err(last_err)
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
}
