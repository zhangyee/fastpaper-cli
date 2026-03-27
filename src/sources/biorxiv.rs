use super::Paper;

/// Search BioRxiv API. BioRxiv only supports browsing by date range,
/// so keyword filtering is done locally after fetching.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let thirty_days = 30 * 24 * 60 * 60;
    let start = now - thirty_days;

    let start_date = format_date(start);
    let end_date = format_date(now);

    let url = format!(
        "{}/details/biorxiv/{}/{}/0",
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

pub fn format_date(secs: u64) -> String {
    let days_since_epoch = secs / 86400;
    let (y, m, d) = days_to_ymd(days_since_epoch);
    format!("{}-{:02}-{:02}", y, m, d)
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Simple conversion from days since epoch to Y-M-D
    let mut y = 1970;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    for days_in_month in &month_days {
        if remaining < *days_in_month {
            break;
        }
        remaining -= days_in_month;
        m += 1;
    }
    (y as u64, m as u64 + 1, remaining as u64 + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Parse BioRxiv JSON search response into a list of Papers.
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

        let category = item["category"]
            .as_str()
            .map(|s| s.to_string());

        let pdf_url = if !doi.is_empty() {
            Some(format!("https://www.biorxiv.org/content/{}v{}.full.pdf", doi, version))
        } else {
            None
        };

        let url = if !doi.is_empty() {
            Some(format!("https://www.biorxiv.org/content/{}v{}", doi, version))
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
            venue: Some("bioRxiv preprint".to_string()),
            citations: None,
            fields: category.into_iter().collect(),
            open_access: Some(true),
            source: "biorxiv".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/biorxiv_search.json");

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
    fn parse_source_is_biorxiv() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "biorxiv");
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
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_authors_split_by_semicolon() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.contains(';'), "author should not contain semicolon: {}", a);
            }
        }
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_category_to_fields() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_fields: Vec<_> = papers.iter().filter(|p| !p.fields.is_empty()).collect();
        assert!(!with_fields.is_empty(), "no papers with fields");
    }

    #[test]
    fn parse_pdf_url_from_doi_and_version() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
        for p in &with_pdf {
            let url = p.pdf_url.as_ref().unwrap();
            assert!(url.contains("biorxiv.org"), "pdf_url should contain biorxiv.org: {}", url);
            assert!(url.ends_with(".full.pdf"), "pdf_url should end with .full.pdf: {}", url);
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
        let papers = search(&server.url(), "adaptation", 3).unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_request_path_contains_details_biorxiv() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/details/biorxiv/".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "adaptation", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_date_range() {
        let mut server = mockito::Server::new();
        // Date range should match YYYY-MM-DD/YYYY-MM-DD pattern
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"\d{4}-\d{2}-\d{2}/\d{4}-\d{2}-\d{2}".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "adaptation", 3);
        mock.assert();
    }

    #[test]
    fn search_filters_by_keyword_locally() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        // "zzz_nonexistent_keyword" should match no papers
        let papers = search(&server.url(), "zzz_nonexistent_keyword", 100).unwrap();
        assert!(papers.is_empty(), "should filter out all papers for non-matching query");
    }
}
