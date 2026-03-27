use super::Paper;

/// Parse Unpaywall JSON response into a Paper.
pub fn parse_response(json: &str) -> Result<Paper, String> {
    let item: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let doi = item["doi"].as_str().unwrap_or("").to_string();
    let title = item["title"].as_str().unwrap_or("").to_string();

    let authors: Vec<String> = item["z_authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    // Try given+family first, fallback to raw_author_name
                    let given = a["given"].as_str().unwrap_or("");
                    let family = a["family"].as_str().unwrap_or("");
                    let combined = format!("{} {}", given, family).trim().to_string();
                    if !combined.is_empty() {
                        Some(combined)
                    } else {
                        a["raw_author_name"].as_str().map(|s| s.to_string())
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let is_oa = item["is_oa"].as_bool();

    let pdf_url = item["best_oa_location"]["url_for_pdf"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let year = item["year"].as_u64().map(|y| y as u16);

    Ok(Paper {
        id: doi.clone(),
        title,
        authors,
        abstract_text: None,
        year,
        doi: Some(doi),
        url: item["doi_url"].as_str().map(|s| s.to_string()),
        pdf_url,
        venue: item["journal_name"].as_str().map(|s| s.to_string()),
        citations: None,
        fields: vec![],
        open_access: is_oa,
        source: "unpaywall".to_string(),
    })
}

/// Lookup a DOI via Unpaywall API.
pub fn lookup_doi(base_url: &str, doi: &str) -> Result<Paper, String> {
    let email = std::env::var("UNPAYWALL_EMAIL")
        .map_err(|_| "UNPAYWALL_EMAIL environment variable is required.\nHint: export UNPAYWALL_EMAIL=\"your@email.com\"".to_string())?;

    let url = format!("{}/v2/{}?email={}", base_url, doi, email);
    let body = match ureq::get(&url).call() {
        Ok(resp) => resp
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response: {}", e))?,
        Err(ureq::Error::StatusCode(404)) => {
            return Err(format!("DOI not found: {}", doi));
        }
        Err(e) => {
            return Err(format!("HTTP error: {}", e));
        }
    };
    parse_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/unpaywall_lookup.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_source_is_unpaywall() {
        let paper = parse_response(FIXTURE).unwrap();
        assert_eq!(paper.source, "unpaywall");
    }

    #[test]
    fn parse_title_not_empty() {
        let paper = parse_response(FIXTURE).unwrap();
        assert!(!paper.title.is_empty());
    }

    #[test]
    fn parse_doi() {
        let paper = parse_response(FIXTURE).unwrap();
        assert_eq!(paper.doi.as_deref(), Some("10.1038/nature12373"));
    }

    #[test]
    fn parse_pdf_url() {
        let paper = parse_response(FIXTURE).unwrap();
        assert!(paper.pdf_url.is_some());
        assert!(paper.pdf_url.as_ref().unwrap().contains(".pdf"));
    }

    #[test]
    fn parse_open_access() {
        let paper = parse_response(FIXTURE).unwrap();
        assert_eq!(paper.open_access, Some(true));
    }

    #[test]
    fn parse_authors() {
        let paper = parse_response(FIXTURE).unwrap();
        assert!(!paper.authors.is_empty());
        for a in &paper.authors {
            assert!(!a.is_empty());
        }
    }

    #[test]
    fn lookup_without_email_returns_err() {
        unsafe { std::env::remove_var("UNPAYWALL_EMAIL") };
        let result = lookup_doi("http://localhost", "10.1038/nature12373");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("UNPAYWALL_EMAIL"), "error should mention env var: {}", err);
    }
}
