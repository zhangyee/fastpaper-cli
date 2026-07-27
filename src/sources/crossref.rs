use super::Paper;

/// Beyond this Crossref requires cursor paging instead of `offset`.
const MAX_OFFSET: u32 = 10_000;

/// Build the /works search URL.
///
/// `mailto` opts into Crossref's polite pool; without it the request goes to
/// the public pool, which still works. Date filters go into a single
/// comma-separated `filter` parameter, which Crossref ANDs together.
fn build_search_url(
    base_url: &str,
    q: &super::SearchQuery,
    email: Option<&str>,
) -> Result<String, String> {
    if q.offset > MAX_OFFSET {
        return Err(format!(
            "crossref supports an offset up to {} (asked for {}); deeper paging needs a cursor",
            MAX_OFFSET, q.offset
        ));
    }

    let mut url = format!(
        "{}/works?query={}&rows={}&offset={}",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        q.offset
    );

    if let Some(ref author) = q.author {
        url.push_str(&format!("&query.author={}", super::encode_query(author)));
    }

    let mut filters = Vec::new();
    match q.year {
        Some(year) => {
            filters.push(format!("from-pub-date:{}-01-01", year));
            filters.push(format!("until-pub-date:{}-12-31", year));
        }
        None => {
            if let Some(ref after) = q.after {
                filters.push(format!("from-pub-date:{}", super::validate_ymd(after)?));
            }
            if let Some(ref before) = q.before {
                filters.push(format!("until-pub-date:{}", super::validate_ymd(before)?));
            }
        }
    }
    if !filters.is_empty() {
        url.push_str(&format!("&filter={}", filters.join(",")));
    }

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "score",
            super::SortField::Date => "published",
            super::SortField::Citations => "is-referenced-by-count",
        };
        let order = match q.order {
            super::SortOrder::Asc => "asc",
            super::SortOrder::Desc => "desc",
        };
        url.push_str(&format!("&sort={}&order={}", by, order));
    }

    if let Some(email) = email {
        url.push_str(&format!("&mailto={}", email));
    }
    Ok(url)
}

fn build_work_url(base_url: &str, doi: &str, email: Option<&str>) -> String {
    let mut url = format!("{}/works/{}", base_url, doi);
    if let Some(email) = email {
        url.push_str(&format!("?mailto={}", email));
    }
    url
}

/// Search CrossRef API.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let email = super::contact_email();
    let url = build_search_url(base_url, q, email.as_deref())?;
    let body = http_get(&url)?;
    parse_search_response(&body)
}

/// Get a single paper by DOI.
pub fn get_by_doi(base_url: &str, doi: &str) -> Result<Option<Paper>, String> {
    let email = super::contact_email();
    let url = build_work_url(base_url, doi, email.as_deref());
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
    use serial_test::serial;

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
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("attention", 3));
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

    // Without FASTPAPER_EMAIL, fall back to Crossref's public pool rather than
    // identifying every user's traffic as the maintainer. Asserted on the built
    // URL rather than through mockito: later-created mocks take precedence
    // there, so an `expect(0)` probe behind a catch-all passes vacuously.
    #[test]
    fn build_search_url_omits_mailto_without_email() {
        let url = build_search_url("https://api.crossref.org", &crate::sources::SearchQuery::simple("attention", 3), None).unwrap();
        assert!(
            !url.contains("mailto="),
            "url should carry no contact address: {}",
            url
        );
    }

    #[test]
    fn build_search_url_includes_mailto_with_email() {
        let url = build_search_url("https://api.crossref.org", &crate::sources::SearchQuery::simple("attention", 3), Some("a@b.com")).unwrap();
        assert!(url.contains("mailto=a@b.com"), "got: {}", url);
    }

    #[test]
    fn build_work_url_omits_mailto_without_email() {
        let url = build_work_url("https://api.crossref.org", "10.1038/nature12373", None);
        assert!(!url.contains("mailto="), "got: {}", url);
    }

    #[test]
    #[serial]
    fn search_sends_env_email_when_set() {
        unsafe { std::env::set_var("FASTPAPER_EMAIL", "custom@example.com") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("mailto=custom".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        unsafe { std::env::remove_var("FASTPAPER_EMAIL") };
        mock.assert();
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField, SortOrder};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://api.crossref.org", q, None).unwrap()
    }

    #[test]
    fn plain_query_uses_query_and_rows() {
        let u = url(&SearchQuery::simple("attention", 10));
        assert!(u.contains("query=attention"), "got: {}", u);
        assert!(u.contains("rows=10"), "got: {}", u);
    }

    #[test]
    fn offset_is_passed_through() {
        let mut q = SearchQuery::simple("attention", 10);
        q.offset = 30;
        assert!(url(&q).contains("offset=30"));
    }

    // Crossref caps offset at 10000 and wants a cursor beyond that.
    #[test]
    fn offset_beyond_the_api_limit_is_rejected() {
        let mut q = SearchQuery::simple("attention", 10);
        q.offset = 10_001;
        let err = build_search_url("https://api.crossref.org", &q, None).unwrap_err();
        assert!(err.contains("10000"), "got: {}", err);
    }

    #[test]
    fn author_uses_the_dedicated_field_query() {
        let mut q = SearchQuery::simple("attention", 10);
        q.author = Some("Vaswani".into());
        assert!(url(&q).contains("query.author=Vaswani"), "got: {}", url(&q));
    }

    #[test]
    fn year_becomes_a_publication_date_filter() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2024);
        let u = url(&q);
        assert!(u.contains("from-pub-date:2024-01-01"), "got: {}", u);
        assert!(u.contains("until-pub-date:2024-12-31"), "got: {}", u);
    }

    #[test]
    fn after_and_before_become_date_filters() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2023-06-01".into());
        q.before = Some("2023-12-31".into());
        let u = url(&q);
        assert!(u.contains("from-pub-date:2023-06-01"), "got: {}", u);
        assert!(u.contains("until-pub-date:2023-12-31"), "got: {}", u);
    }

    #[test]
    fn multiple_filters_are_comma_separated_in_one_parameter() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2023-06-01".into());
        q.before = Some("2023-12-31".into());
        let u = url(&q);
        assert_eq!(u.matches("filter=").count(), 1, "one filter parameter: {}", u);
        assert!(u.contains(','), "filters are comma separated: {}", u);
    }

    #[test]
    fn malformed_date_is_rejected() {
        let mut q = SearchQuery::simple("attention", 10);
        q.before = Some("2023/12/31".into());
        let err = build_search_url("https://api.crossref.org", &q, None).unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "got: {}", err);
    }

    #[test]
    fn sort_by_citations_uses_the_reference_count() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Citations);
        let u = url(&q);
        assert!(u.contains("sort=is-referenced-by-count"), "got: {}", u);
        assert!(u.contains("order=desc"), "got: {}", u);
    }

    #[test]
    fn sort_by_date_uses_published() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Date);
        assert!(url(&q).contains("sort=published"));
    }

    #[test]
    fn sort_by_relevance_uses_score() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Relevance);
        assert!(url(&q).contains("sort=score"));
    }

    #[test]
    fn ascending_order_is_honoured() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(url(&q).contains("order=asc"));
    }

    #[test]
    fn mailto_is_still_appended_when_supplied() {
        let q = SearchQuery::simple("attention", 10);
        let u = build_search_url("https://api.crossref.org", &q, Some("a@b.com")).unwrap();
        assert!(u.contains("mailto=a@b.com"), "got: {}", u);
    }
}
