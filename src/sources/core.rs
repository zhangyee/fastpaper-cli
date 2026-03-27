use super::Paper;

/// Search CORE API.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let encoded = super::encode_query(query);
    let url = format!(
        "{}/search/works?q={}&limit={}",
        base_url, encoded, max_results
    );
    let api_key = std::env::var("CORE_API_KEY").ok();

    // Try with key first, then without on 403
    let result = http_get_core(&url, api_key.as_deref());
    match result {
        Err(ref e) if e.contains("403") && api_key.is_some() => {
            // Retry without key
            let body = http_get_core(&url, None)?;
            parse_search_response(&body)
        }
        Err(e) => Err(e),
        Ok(body) => parse_search_response(&body),
    }
}

fn http_get_core(url: &str, api_key: Option<&str>) -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        let mut req = ureq::get(url);
        if let Some(key) = api_key {
            req = req.header("Authorization", &format!("Bearer {}", key));
        }
        match req.call() {
            Ok(resp) => {
                return resp
                    .into_body()
                    .read_to_string()
                    .map_err(|e| format!("Failed to read response: {}", e));
            }
            Err(ureq::Error::StatusCode(429)) => {
                last_err = "rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(403)) => {
                return Err("403 Forbidden".to_string());
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

/// Parse CORE JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array")?;

    let mut papers = Vec::new();
    for item in results {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let id = item["id"].as_u64().map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());
        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        let download_url = item["downloadUrl"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let citations = item["citationCount"].as_u64().map(|n| n as u32);

        let year = item["publishedDate"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok())
            .or_else(|| item["yearPublished"].as_u64().map(|y| y as u16));

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["links"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|l| l["url"].as_str())
                .map(|s| s.to_string()),
            pdf_url: download_url,
            venue: None,
            citations,
            fields: item["fieldOfStudy"]
                .as_str()
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
            open_access: Some(true),
            source: "core".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const FIXTURE: &str = include_str!("../../tests/fixtures/core_search.json");

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
    fn parse_source_is_core() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "core");
        }
    }

    #[test]
    fn parse_title_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_pdf_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
        }
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
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/search/works".to_string()))
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
    fn search_with_api_key_sends_bearer() {
        unsafe { std::env::set_var("CORE_API_KEY", "core-test-key") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("Authorization", "Bearer core-test-key")
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(&server.url(), "test", 3);
        unsafe { std::env::remove_var("CORE_API_KEY") };
        assert!(result.is_ok());
        mock.assert();
    }

    #[test]
    #[serial]
    fn search_works_without_api_key() {
        unsafe { std::env::remove_var("CORE_API_KEY") };
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(&server.url(), "test", 3);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn search_403_with_key_retries_without() {
        unsafe { std::env::set_var("CORE_API_KEY", "bad-key") };
        let mut server = mockito::Server::new();
        // First request with key → 403
        server
            .mock("GET", mockito::Matcher::Any)
            .match_header("Authorization", "Bearer bad-key")
            .with_status(403)
            .create();
        // Second request without key → 200
        server
            .mock("GET", mockito::Matcher::Any)
            .match_header("Authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(&server.url(), "test", 3);
        unsafe { std::env::remove_var("CORE_API_KEY") };
        assert!(result.is_ok());
    }
}
