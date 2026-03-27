use super::Paper;

const DEFAULT_EMAIL: &str = "yee.zhang@gmail.com";

/// Search CrossRef API.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let mailto = std::env::var("FASTPAPER_EMAIL").unwrap_or_else(|_| DEFAULT_EMAIL.to_string());
    let encoded = super::encode_query(query);
    let url = format!(
        "{}/works?query={}&rows={}&mailto={}",
        base_url, encoded, max_results, mailto
    );
    let body = http_get(&url)?;
    parse_search_response(&body)
}

/// Get a single paper by DOI.
pub fn get_by_doi(base_url: &str, doi: &str) -> Result<Option<Paper>, String> {
    let mailto = std::env::var("FASTPAPER_EMAIL").unwrap_or_else(|_| DEFAULT_EMAIL.to_string());
    let url = format!("{}/works/{}?mailto={}", base_url, doi, mailto);
    match ureq::get(&url).call() {
        Ok(resp) => {
            let body = resp
                .into_body()
                .read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            let root: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {}", e))?;
            let item = &root["message"];
            if item.is_null() {
                return Ok(None);
            }
            Ok(Some(parse_crossref_item(item)))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(ureq::Error::StatusCode(429)) => Err("rate limited (429)".to_string()),
        Err(ureq::Error::StatusCode(code)) if code >= 500 => {
            Err(format!("Server error: {}", code))
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

fn http_get(url: &str) -> Result<String, String> {
    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        match ureq::get(url).call() {
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

/// Parse CrossRef JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let items = root["message"]["items"]
        .as_array()
        .ok_or("missing 'message.items' array")?;

    Ok(items.iter().map(parse_crossref_item).collect())
}

fn parse_crossref_item(item: &serde_json::Value) -> Paper {
    let doi = item["DOI"].as_str().unwrap_or("").to_string();
    let title = item["title"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let authors: Vec<String> = item["author"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| {
                    let given = a["given"].as_str().unwrap_or("");
                    let family = a["family"].as_str().unwrap_or("");
                    format!("{} {}", given, family).trim().to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    let year = extract_year(item, "published")
        .or_else(|| extract_year(item, "issued"))
        .or_else(|| extract_year(item, "created"));

    let url = item["URL"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://doi.org/{}", doi));

    let venue = item["container-title"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let citations = item["is-referenced-by-count"].as_u64().map(|n| n as u32);

    let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

    Paper {
        id: doi.clone(),
        title,
        authors,
        abstract_text,
        year,
        doi: Some(doi),
        url: Some(url),
        pdf_url: None,
        venue,
        citations,
        fields: vec![],
        open_access: None,
        source: "crossref".to_string(),
    }
}

fn extract_year(item: &serde_json::Value, field: &str) -> Option<u16> {
    item[field]["date-parts"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|parts| parts.as_array())
        .and_then(|parts| parts.first())
        .and_then(|y| y.as_u64())
        .map(|y| y as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/crossref_search.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_titles_from_array() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_given_family() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // At least one paper should have authors
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.is_empty(), "empty author name in paper {}", p.id);
            }
        }
    }

    #[test]
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.doi.is_some(), "paper missing DOI");
            assert!(!p.doi.as_ref().unwrap().is_empty());
        }
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
        }
    }

    #[test]
    fn parse_year_from_date_parts() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() > 2000);
        }
    }

    #[test]
    fn parse_source_is_crossref() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "crossref");
        }
    }

    #[test]
    fn search_request_path_is_works() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"/works\?".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "attention", 3);
        mock.assert();
    }

    // Single-item response for get_by_doi tests
    fn single_item_response() -> String {
        // Wrap the first item from fixture as a single-item response
        let root: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        let first_item = &root["message"]["items"][0];
        serde_json::json!({
            "status": "ok",
            "message": first_item,
        })
        .to_string()
    }

    #[test]
    fn get_by_doi_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex(r"/works/10\.1038/nature12373".to_string()),
            )
            .with_status(200)
            .with_body(single_item_response())
            .create();
        let _ = get_by_doi(&server.url(), "10.1038/nature12373");
        mock.assert();
    }

    #[test]
    fn get_by_doi_returns_paper() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(single_item_response())
            .create();
        let result = get_by_doi(&server.url(), "10.1038/nature12373").unwrap();
        assert!(result.is_some());
        let paper = result.unwrap();
        assert_eq!(paper.source, "crossref");
        mock.assert();
    }

    #[test]
    fn get_by_doi_returns_none_on_404() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let result = get_by_doi(&server.url(), "10.9999/nonexistent").unwrap();
        assert!(result.is_none());
        mock.assert();
    }

    #[test]
    fn search_request_contains_mailto() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("mailto=".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }
}
