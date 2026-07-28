use super::Paper;

/// Search CORE API.
/// Build the /v3/search/works URL.
///
/// CORE takes field-scoped terms inside `q` and pages with limit/offset.
///
/// The author field is `authors.name`, and the value must be quoted: bare
/// `authors.name:Doudna` matches nothing, and the shorter `authors:"Doudna"`
/// silently *widens* the result set (60460 -> 874397 for the same query)
/// instead of narrowing it.
///
/// The trailing slash matters too -- `/v3/search/works` answers 301 to
/// `/v3/search/works/`.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    if q.limit > MAX_LIMIT {
        return Err(format!(
            "core returns at most {} results per request (asked for {}); \
             larger limits time out server-side",
            MAX_LIMIT, q.limit
        ));
    }
    let mut terms = vec![super::encode_query(&q.query)];
    if let Some(ref author) = q.author {
        terms.push(format!("authors.name:%22{}%22", super::encode_query(author)));
    }
    if let Some(year) = q.year {
        terms.push(format!("yearPublished:{}", year));
    }
    Ok(format!(
        "{}/v3/search/works/?q={}&limit={}&offset={}",
        base_url,
        terms.join("+AND+"),
        q.limit,
        q.offset
    ))
}

/// Beyond this CORE answers 504; 100 is the largest verified working value.
const MAX_LIMIT: u32 = 100;

/// Fetch a single work by CORE id, DOI or other supported identifier.
///
/// `GET /v3/works/{identifier}` returns one work object rather than the
/// `{results: [...]}` envelope the search endpoint uses.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!("{}/v3/works/{}", base_url, super::encode_query(identifier));
    let api_key = std::env::var("CORE_API_KEY").ok();
    match http_get_core(&url, api_key.as_deref()) {
        Ok(body) => {
            // parse_search_response expects the list envelope.
            let wrapped = format!(r#"{{"results":[{}]}}"#, body);
            Ok(parse_search_response(&wrapped)?.into_iter().next())
        }
        Err(e) if e.contains("404") => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;
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
        let papers = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3)).unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/v3/search/works".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
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
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
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
        let result = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
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
        let result = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
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
        let result = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        unsafe { std::env::remove_var("CORE_API_KEY") };
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod get_tests {
    use super::*;

    // /v3/works/{id} answers with a bare work object, not the search envelope.
    const WORK: &str = r#"{"id":12345678,"title":"A CORE work",
"authors":[{"name":"Jane Doe"}],"yearPublished":2021,
"doi":"10.1234/abcd","abstract":"Some abstract.",
"downloadUrl":"https://core.ac.uk/download/12345678.pdf"}"#;

    #[test]
    fn get_by_id_parses_a_single_work() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("/v3/works/".to_string()))
            .with_status(200)
            .with_body(WORK)
            .create();
        let paper = get_by_id(&server.url(), "12345678").unwrap().unwrap();
        assert_eq!(paper.title, "A CORE work");
        assert_eq!(paper.year, Some(2021));
        assert_eq!(paper.source, "core");
    }

    #[test]
    fn get_by_id_encodes_a_doi_identifier() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/v3/works/10".to_string()))
            .with_status(200)
            .with_body(WORK)
            .create();
        let _ = get_by_id(&server.url(), "10.1234/abcd");
        mock.assert();
    }

    #[test]
    fn get_by_id_returns_none_on_404() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        assert!(get_by_id(&server.url(), "nope").unwrap().is_none());
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;
    use crate::sources::SearchQuery;

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://api.core.ac.uk", q).unwrap()
    }

    // /v3/search/works answers 301 to the trailing-slash form.
    #[test]
    fn search_path_carries_the_trailing_slash() {
        assert!(url(&SearchQuery::simple("crispr", 10)).contains("/v3/search/works/?"));
    }

    // `authors:"X"` widens the result set instead of narrowing it; the field is
    // authors.name, and the value has to be quoted or it matches nothing.
    #[test]
    fn author_uses_the_quoted_authors_name_field() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Doudna".into());
        let u = url(&q);
        assert!(u.contains("authors.name:%22Doudna%22"), "got: {}", u);
        assert!(!u.contains("authors:%22"), "wrong field widens results: {}", u);
    }

    #[test]
    fn year_uses_year_published() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2020);
        assert!(url(&q).contains("yearPublished:2020"), "got: {}", url(&q));
    }

    #[test]
    fn offset_is_passed_through() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 30;
        assert!(url(&q).contains("offset=30"));
    }

    // 200 already times out server-side with a 504.
    #[test]
    fn limit_above_the_cap_is_rejected() {
        let err = build_search_url("https://api.core.ac.uk", &SearchQuery::simple("x", 200))
            .unwrap_err();
        assert!(err.contains("100"), "got: {}", err);
    }

    #[test]
    fn limit_at_the_cap_is_allowed() {
        assert!(build_search_url("https://api.core.ac.uk", &SearchQuery::simple("x", 100)).is_ok());
    }
}
