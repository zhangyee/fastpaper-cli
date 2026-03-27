use super::Paper;

/// Search MedRxiv API. Same as BioRxiv but with different URL path.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let thirty_days = 30 * 24 * 60 * 60;
    let start = now - thirty_days;
    let start_date = super::biorxiv::format_date(start);
    let end_date = super::biorxiv::format_date(now);

    let url = format!(
        "{}/details/medrxiv/{}/{}/0",
        base_url, start_date, end_date
    );

    let body = http_get(&url)?;
    let all_papers = parse_search_response(&body)?;

    let query_lower = query.to_lowercase();
    let filtered: Vec<Paper> = all_papers
        .into_iter()
        .filter(|p| {
            p.title.to_lowercase().contains(&query_lower)
                || p.abstract_text
                    .as_ref()
                    .map(|a| a.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .take(max_results as usize)
        .collect();

    Ok(filtered)
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

/// Parse MedRxiv JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let collection = root["collection"]
        .as_array()
        .ok_or("missing 'collection' array")?;

    let mut papers = Vec::new();
    for item in collection {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let doi = item["doi"].as_str().unwrap_or("").to_string();
        let version = item["version"].as_str().unwrap_or("1");

        let authors: Vec<String> = item["authors"]
            .as_str()
            .map(|s| s.split(';').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect())
            .unwrap_or_default();

        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        let year = item["date"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let category = item["category"].as_str().map(|s| s.to_string());

        let pdf_url = if !doi.is_empty() {
            Some(format!("https://www.medrxiv.org/content/{}v{}.full.pdf", doi, version))
        } else {
            None
        };

        let url = if !doi.is_empty() {
            Some(format!("https://www.medrxiv.org/content/{}v{}", doi, version))
        } else {
            None
        };

        papers.push(Paper {
            id: doi.clone(),
            title,
            authors,
            abstract_text,
            year,
            doi: if doi.is_empty() { None } else { Some(doi) },
            url,
            pdf_url,
            venue: Some("medRxiv preprint".to_string()),
            citations: None,
            fields: category.into_iter().collect(),
            open_access: Some(true),
            source: "medrxiv".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/medrxiv_search.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_source_is_medrxiv() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "medrxiv");
        }
    }

    #[test]
    fn search_request_path_contains_details_medrxiv() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/details/medrxiv/".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "risk", 3);
        mock.assert();
    }

    #[test]
    fn parse_pdf_url_is_medrxiv_domain() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty());
        for p in &with_pdf {
            assert!(p.pdf_url.as_ref().unwrap().contains("www.medrxiv.org"));
        }
    }

    #[test]
    fn parse_venue_is_medrxiv_preprint() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.venue.as_deref(), Some("medRxiv preprint"));
        }
    }
}
