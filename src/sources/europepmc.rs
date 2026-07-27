use super::Paper;

/// Europe PMC's REST service lives under this path on the EBI host; `base_url`
/// is the bare host, matching the convention used by the other sources.
const SEARCH_PATH: &str = "/europepmc/webservices/rest/search";

/// Build the search URL.
///
/// Europe PMC puts everything in the query string itself: filters as
/// `FIELD:value` terms and ordering as `sort_date:y` / `sort_cited:y`. The
/// separate `sort=` parameter is documented but returns non-JSON for
/// `P_PDATE_D desc` and `CITED desc`, so the in-query form is what we use — and
/// it only orders descending.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    if q.limit > 0 && q.offset % q.limit != 0 {
        return Err(format!(
            "europepmc pages results, so --offset must be a multiple of -n \
             (got --offset {} with -n {})",
            q.offset, q.limit
        ));
    }
    let page = if q.limit > 0 { q.offset / q.limit + 1 } else { 1 };

    let mut terms = vec![super::encode_query(&q.query)];

    if let Some(ref author) = q.author {
        terms.push(format!("AUTH:%22{}%22", super::encode_query(author)));
    }
    match q.year {
        Some(year) => terms.push(format!("PUB_YEAR:{}", year)),
        None => {
            if q.after.is_some() || q.before.is_some() {
                let from = match q.after {
                    Some(ref d) => super::validate_ymd(d)?,
                    None => "1800-01-01",
                };
                let to = match q.before {
                    Some(ref d) => super::validate_ymd(d)?,
                    None => "3000-12-31",
                };
                terms.push(format!("FIRST_PDATE:%5B{}+TO+{}%5D", from, to));
            }
        }
    }
    if q.open_access {
        terms.push("OPEN_ACCESS:y".to_string());
    }

    let mut query = terms.join("+AND+");

    if let Some(sort) = q.sort {
        if q.order == super::SortOrder::Asc {
            return Err(
                "europepmc only orders results newest/most-cited first; --order asc is unavailable"
                    .to_string(),
            );
        }
        match sort {
            // Relevance is the default ranking.
            super::SortField::Relevance => {}
            super::SortField::Date => query.push_str("+sort_date:y"),
            super::SortField::Citations => query.push_str("+sort_cited:y"),
        }
    }

    Ok(format!(
        "{}{}?query={}&pageSize={}&page={}&format=json&resultType=core",
        base_url, SEARCH_PATH, query, q.limit, page
    ))
}

/// Build the query that pins one record by identifier.
///
/// Europe PMC has no by-id path; a field-scoped search is the documented way,
/// and which field depends on the identifier's shape.
fn id_query(id: &str) -> String {
    if id.starts_with("PMC") {
        format!("PMCID:{}", id)
    } else if id.starts_with("10.") {
        format!("DOI:%22{}%22", super::encode_query(id))
    } else {
        format!("EXT_ID:{}+AND+SRC:MED", super::encode_query(id))
    }
}

/// Fetch a single record by PMID, PMC ID or DOI.
pub fn get_by_id(base_url: &str, id: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}{}?query={}&pageSize=1&format=json&resultType=core",
        base_url,
        SEARCH_PATH,
        id_query(id)
    );
    let body = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok(parse_search_response(&body)?.into_iter().next())
}

/// Search Europe PMC API.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        match ureq::get(&url).call() {
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

/// Parse Europe PMC JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["resultList"]["result"]
        .as_array()
        .ok_or("missing 'resultList.result' array")?;

    let mut papers = Vec::new();
    for item in results {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let id = item["id"].as_str().unwrap_or("").to_string();

        let authors: Vec<String> = item["authorList"]["author"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["fullName"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());
        let abstract_text = item["abstractText"].as_str().map(|s| s.to_string());
        let year = item["pubYear"]
            .as_str()
            .and_then(|s| s.parse::<u16>().ok());
        let citations = item["citedByCount"].as_u64().map(|n| n as u32);

        let is_oa = item["isOpenAccess"]
            .as_str()
            .map(|s| s == "Y");

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["fullTextUrlList"]["fullTextUrl"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|u| u["url"].as_str())
                .map(|s| s.to_string()),
            // fullTextUrlList marks the PDF rendering with documentStyle "pdf".
            pdf_url: item["fullTextUrlList"]["fullTextUrl"]
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .find(|u| u["documentStyle"].as_str() == Some("pdf"))
                        .and_then(|u| u["url"].as_str())
                })
                .map(|s| s.to_string()),
            venue: item["journalTitle"].as_str().map(|s| s.to_string()),
            citations,
            fields: vec![],
            open_access: is_oa,
            source: "europepmc".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/europepmc_search.json");

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
    fn parse_source_is_europepmc() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "europepmc");
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
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
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
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_oa: Vec<_> = papers.iter().filter(|p| p.open_access.is_some()).collect();
        assert!(!with_oa.is_empty(), "no papers with open_access");
        // open_access should be a bool, not a string
        for p in &with_oa {
            let _ = p.open_access.unwrap(); // just confirm it's bool
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
    fn search_request_path_contains_search() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/search".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_page_size() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("pageSize=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_format_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("format=json".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    // The default base URL is a bare host; the source must append Europe PMC's
    // full REST path. Matching only "/search" let a production-404 URL pass.
    #[test]
    fn search_request_uses_europepmc_rest_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/europepmc/webservices/rest/search".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_result_type_core() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("resultType=core".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField, SortOrder};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://www.ebi.ac.uk", q).unwrap()
    }

    #[test]
    fn author_becomes_an_auth_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Doudna".into());
        assert!(url(&q).contains("AUTH:%22Doudna%22"), "got: {}", url(&q));
    }

    #[test]
    fn year_becomes_a_pub_year_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        assert!(url(&q).contains("PUB_YEAR:2024"), "got: {}", url(&q));
    }

    #[test]
    fn dates_become_a_first_pdate_range() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.after = Some("2024-01-01".into());
        q.before = Some("2024-03-31".into());
        assert!(
            url(&q).contains("FIRST_PDATE:%5B2024-01-01+TO+2024-03-31%5D"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn open_access_becomes_a_query_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.open_access = true;
        assert!(url(&q).contains("OPEN_ACCESS:y"), "got: {}", url(&q));
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 20;
        assert!(url(&q).contains("page=3"), "got: {}", url(&q));
    }

    #[test]
    fn offset_that_is_not_a_whole_page_is_rejected() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 15;
        let err = build_search_url("https://www.ebi.ac.uk", &q).unwrap_err();
        assert!(err.contains("multiple of"), "got: {}", err);
    }

    // Ordering is expressed in the query string, not a sort parameter.
    #[test]
    fn sort_by_citations_appends_sort_cited() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Citations);
        assert!(url(&q).contains("sort_cited:y"), "got: {}", url(&q));
    }

    #[test]
    fn sort_by_date_appends_sort_date() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Date);
        assert!(url(&q).contains("sort_date:y"), "got: {}", url(&q));
    }

    #[test]
    fn ascending_order_is_rejected() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        let err = build_search_url("https://www.ebi.ac.uk", &q).unwrap_err();
        assert!(err.contains("--order asc"), "got: {}", err);
    }

    #[test]
    fn the_rest_path_is_still_the_full_one() {
        assert!(url(&SearchQuery::simple("crispr", 10))
            .contains("/europepmc/webservices/rest/search"));
    }
}
