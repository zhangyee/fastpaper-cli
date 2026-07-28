use super::Paper;

/// Default window when the caller gives no date filter.
const DEFAULT_WINDOW_DAYS: u64 = 30;
/// Records the API returns per cursor step.
const PAGE_SIZE: u32 = 30;
/// How many cursor steps to walk before giving up. Keyword matching happens
/// locally, so without a bound a rare term would page the entire archive.
const MAX_PAGES: u32 = 20;

/// Resolve the date window to browse, in `YYYY-MM-DD` form.
///
/// bioRxiv has no keyword search — only browsing an interval — so the date
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

/// Fetch a single preprint by DOI.
///
/// `/details/{server}/{doi}` is the one by-identifier endpoint this API has,
/// and it answers with the same `{collection: [...]}` shape as the interval
/// browse, so the parser is reused.
pub fn get_by_id(base_url: &str, doi: &str) -> Result<Option<Paper>, String> {
    let url = format!("{}/details/biorxiv/{}", base_url, doi);
    let body = http_get(&url)?;
    Ok(parse_search_response(&body)?.into_iter().next())
}

/// Search bioRxiv. The API only browses by date range, so the keyword is
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
            "{}/details/biorxiv/{}/{}/{}",
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
            .map(|s| {
                s.split(';')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        let year = item["date"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let category = item["category"].as_str().map(|s| s.to_string());

        let pdf_url = if !doi.is_empty() {
            Some(format!(
                "https://www.biorxiv.org/content/{}v{}.full.pdf",
                doi, version
            ))
        } else {
            None
        };

        let url = if !doi.is_empty() {
            Some(format!(
                "https://www.biorxiv.org/content/{}v{}",
                doi, version
            ))
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
                assert!(
                    !a.contains(';'),
                    "author should not contain semicolon: {}",
                    a
                );
            }
        }
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers
            .iter()
            .filter(|p| p.abstract_text.is_some())
            .collect();
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
            assert!(
                url.contains("biorxiv.org"),
                "pdf_url should contain biorxiv.org: {}",
                url
            );
            assert!(
                url.ends_with(".full.pdf"),
                "pdf_url should end with .full.pdf: {}",
                url
            );
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
        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("adaptation", 3),
        )
        .unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_request_path_contains_details_biorxiv() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/details/biorxiv/".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("adaptation", 3),
        );
        mock.assert();
    }

    #[test]
    fn search_request_contains_date_range() {
        let mut server = mockito::Server::new();
        // Date range should match YYYY-MM-DD/YYYY-MM-DD pattern
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex(r"\d{4}-\d{2}-\d{2}/\d{4}-\d{2}-\d{2}".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("adaptation", 3),
        );
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
        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("zzz_nonexistent_keyword", 100),
        )
        .unwrap();
        assert!(
            papers.is_empty(),
            "should filter out all papers for non-matching query"
        );
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use crate::sources::SearchQuery;

    // 2024-07-01T00:00:00Z
    const NOW: u64 = 1_719_792_000;

    #[test]
    fn no_date_filter_browses_the_recent_window() {
        let (start, end) = resolve_window(&SearchQuery::simple("x", 10), NOW).unwrap();
        assert_eq!(end, "2024-07-01");
        assert_eq!(start, "2024-06-01", "30 days back");
    }

    // The date filters are the only part of the query bioRxiv can honour
    // natively; they choose the slice the keyword is then matched against.
    #[test]
    fn year_browses_the_whole_year() {
        let mut q = SearchQuery::simple("x", 10);
        q.year = Some(2022);
        assert_eq!(
            resolve_window(&q, NOW).unwrap(),
            ("2022-01-01".to_string(), "2022-12-31".to_string())
        );
    }

    #[test]
    fn after_and_before_set_both_ends() {
        let mut q = SearchQuery::simple("x", 10);
        q.after = Some("2023-03-01".into());
        q.before = Some("2023-04-01".into());
        assert_eq!(
            resolve_window(&q, NOW).unwrap(),
            ("2023-03-01".to_string(), "2023-04-01".to_string())
        );
    }

    #[test]
    fn after_alone_runs_to_today() {
        let mut q = SearchQuery::simple("x", 10);
        q.after = Some("2023-03-01".into());
        let (start, end) = resolve_window(&q, NOW).unwrap();
        assert_eq!(start, "2023-03-01");
        assert_eq!(end, "2024-07-01");
    }

    #[test]
    fn malformed_date_is_rejected() {
        let mut q = SearchQuery::simple("x", 10);
        q.after = Some("last March".into());
        assert!(resolve_window(&q, NOW).unwrap_err().contains("YYYY-MM-DD"));
    }

    #[test]
    fn year_wins_over_a_date_range() {
        let mut q = SearchQuery::simple("x", 10);
        q.year = Some(2022);
        q.after = Some("2023-03-01".into());
        assert_eq!(resolve_window(&q, NOW).unwrap().0, "2022-01-01");
    }
}

#[cfg(test)]
mod get_tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/biorxiv_search.json");

    // The one by-identifier endpoint this API has; same envelope as browsing.
    #[test]
    fn get_by_id_hits_the_details_endpoint() {
        let mut server = mockito::Server::new();
        let m = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/details/biorxiv/10".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = get_by_id(&server.url(), "10.1101/2020.01.30.927871");
        m.assert();
    }

    #[test]
    fn get_by_id_returns_the_first_preprint() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        assert!(get_by_id(&server.url(), "10.1101/x").unwrap().is_some());
    }
}
