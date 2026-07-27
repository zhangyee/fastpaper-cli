use super::Paper;

use super::biorxiv::format_date;

/// Default window when the caller gives no date filter.
const DEFAULT_WINDOW_DAYS: u64 = 30;
/// Records the API returns per cursor step.
const PAGE_SIZE: u32 = 30;
/// How many cursor steps to walk before giving up. Keyword matching happens
/// locally, so without a bound a rare term would page the entire archive.
const MAX_PAGES: u32 = 20;

/// Resolve the date window to browse, in `YYYY-MM-DD` form.
///
/// medRxiv has no keyword search — only browsing an interval — so the date
/// filters are the one part of the query it can honour natively, and they
/// decide which slice we then match against locally.
pub fn resolve_window(q: &super::SearchQuery, now_secs: u64) -> Result<(String, String), String> {
    if let Some(year) = q.year {
        return Ok((format!("{}-01-01", year), format!("{}-12-31", year)));
    }
    let start = match q.after {
        Some(ref d) => super::validate_ymd(d)?.to_string(),
        None => format_date(now_secs - DEFAULT_WINDOW_DAYS * 86400),
    };
    let end = match q.before {
        Some(ref d) => super::validate_ymd(d)?.to_string(),
        None => format_date(now_secs),
    };
    Ok((start, end))
}

/// Search medRxiv. The API only browses by date range, so the keyword is
/// matched locally over the records in the window.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (start_date, end_date) = resolve_window(q, now)?;

    let query_lower = q.query.to_lowercase();
    let wanted = (q.offset + q.limit) as usize;
    let mut matched: Vec<Paper> = Vec::new();

    // Walk the cursor until enough matches accumulate, the window runs out, or
    // the page budget is spent.
    for page in 0..MAX_PAGES {
        let url = format!(
            "{}/details/medrxiv/{}/{}/{}",
            base_url,
            start_date,
            end_date,
            page * PAGE_SIZE
        );
        let body = http_get(&url)?;
        let batch = parse_search_response(&body)?;
        let exhausted = batch.len() < PAGE_SIZE as usize;

        matched.extend(batch.into_iter().filter(|p| {
            p.title.to_lowercase().contains(&query_lower)
                || p.abstract_text
                    .as_ref()
                    .map(|a| a.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        }));

        if matched.len() >= wanted || exhausted {
            break;
        }
    }

    Ok(matched
        .into_iter()
        .skip(q.offset as usize)
        .take(q.limit as usize)
        .collect())
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
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("risk", 3));
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
