use super::Paper;

/// Build the /works search URL.
///
/// OpenAlex takes filters as one comma-separated `filter` parameter and pages
/// results rather than offsetting them. Since 2026-02 it meters usage: requests
/// still work anonymously on a small daily budget, and `OPENALEX_API_KEY` buys
/// the larger free tier.
fn build_search_url(
    base_url: &str,
    q: &super::SearchQuery,
    api_key: Option<&str>,
    email: Option<&str>,
) -> Result<String, String> {
    if q.limit > 0 && q.offset % q.limit != 0 {
        return Err(format!(
            "openalex pages results rather than offsetting them, so --offset must be a multiple of -n \
             (got --offset {} with -n {})",
            q.offset, q.limit
        ));
    }
    let page = if q.limit > 0 { q.offset / q.limit + 1 } else { 1 };

    let mut url = format!(
        "{}/works?search={}&per_page={}&page={}",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        page
    );

    let mut filters = Vec::new();
    if let Some(ref author) = q.author {
        filters.push(format!(
            "raw_author_name.search:{}",
            super::encode_query(author)
        ));
    }
    match q.year {
        Some(year) => filters.push(format!("publication_year:{}", year)),
        None => {
            if let Some(ref after) = q.after {
                filters.push(format!("from_publication_date:{}", super::validate_ymd(after)?));
            }
            if let Some(ref before) = q.before {
                filters.push(format!("to_publication_date:{}", super::validate_ymd(before)?));
            }
        }
    }
    if q.open_access {
        filters.push("is_oa:true".to_string());
    }
    // OpenAlex has no name-based subject filter -- concepts.display_name.search
    // and primary_topic.field.display_name are both rejected -- so --field takes
    // a concept ID.
    if let Some(ref field) = q.field {
        filters.push(format!("concepts.id:{}", super::encode_query(field)));
    }
    if !filters.is_empty() {
        url.push_str(&format!("&filter={}", filters.join(",")));
    }

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "relevance_score",
            super::SortField::Date => "publication_date",
            super::SortField::Citations => "cited_by_count",
        };
        let order = match q.order {
            super::SortOrder::Asc => "asc",
            super::SortOrder::Desc => "desc",
        };
        url.push_str(&format!("&sort={}:{}", by, order));
    }

    if let Some(key) = api_key {
        url.push_str(&format!("&api_key={}", key));
    }
    if let Some(email) = email {
        url.push_str(&format!("&mailto={}", email));
    }
    Ok(url)
}

/// Fetch a single work by DOI or OpenAlex ID.
///
/// Single-record lookups are the cheapest call OpenAlex offers, so this stays
/// usable without a key.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let path = if identifier.starts_with("10.") {
        format!("doi:{}", identifier)
    } else {
        identifier.to_string()
    };
    let mut url = format!("{}/works/{}", base_url, path);
    if let Ok(key) = std::env::var("OPENALEX_API_KEY") {
        url.push_str(&format!("?api_key={}", key));
    }
    match ureq::get(&url).call() {
        Ok(resp) => {
            let body = resp
                .into_body()
                .read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            // parse_search_response expects the list shape.
            let wrapped = format!(r#"{{"results":[{}]}}"#, body);
            Ok(parse_search_response(&wrapped)?.into_iter().next())
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(ureq::Error::StatusCode(429)) => Err("rate limited (429)".to_string()),
        Err(ureq::Error::StatusCode(code)) if code >= 500 => Err(format!("Server error: {}", code)),
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Search OpenAlex API.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let api_key = std::env::var("OPENALEX_API_KEY").ok();
    let email = super::contact_email();
    let url = build_search_url(base_url, q, api_key.as_deref(), email.as_deref())?;

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

/// Parse OpenAlex JSON search response into a list of Papers.
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

        let paper_id = item["id"]
            .as_str()
            .unwrap_or("")
            .replace("https://openalex.org/", "");

        let authors: Vec<String> = item["authorships"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["author"]["display_name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"]
            .as_str()
            .map(|s| s.replace("https://doi.org/", ""));

        let abstract_text = reconstruct_abstract(&item["abstract_inverted_index"]);

        let year = item["publication_year"].as_u64().map(|y| y as u16);
        let citations = item["cited_by_count"].as_u64().map(|n| n as u32);
        let is_oa = item["open_access"]["is_oa"].as_bool();

        let url = item["primary_location"]["landing_page_url"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .map(|s| s.to_string());

        let pdf_url = item["primary_location"]["pdf_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                if is_oa == Some(true) {
                    item["open_access"]["oa_url"].as_str().filter(|s| !s.is_empty())
                } else {
                    None
                }
            })
            .map(|s| s.to_string());

        let venue = item["primary_location"]["source"]["display_name"]
            .as_str()
            .map(|s| s.to_string());

        let fields: Vec<String> = item["concepts"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(5)
                    .filter_map(|c| c["display_name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        papers.push(Paper {
            id: paper_id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url,
            pdf_url,
            venue,
            citations,
            fields,
            open_access: is_oa,
            source: "openalex".to_string(),
        });
    }

    Ok(papers)
}

/// Reconstruct abstract text from OpenAlex inverted index format.
fn reconstruct_abstract(inverted_index: &serde_json::Value) -> Option<String> {
    let obj = inverted_index.as_object()?;
    let mut word_positions: Vec<(u64, &str)> = Vec::new();
    for (word, positions) in obj {
        if let Some(arr) = positions.as_array() {
            for pos in arr {
                if let Some(p) = pos.as_u64() {
                    word_positions.push((p, word));
                }
            }
        }
    }
    if word_positions.is_empty() {
        return None;
    }
    word_positions.sort_by_key(|(pos, _)| *pos);
    Some(word_positions.iter().map(|(_, word)| *word).collect::<Vec<&str>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openalex_search.json");

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
    fn parse_source_is_openalex() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "openalex");
        }
    }

    #[test]
    fn parse_titles_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_from_authorships() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.authors.is_empty(), "paper {} has no authors", p.id);
        }
    }

    #[test]
    fn parse_doi_strips_prefix() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            if let Some(ref doi) = p.doi {
                assert!(
                    !doi.starts_with("https://"),
                    "DOI should not have URL prefix: {}",
                    doi
                );
            }
        }
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
            assert!(p.citations.unwrap() > 0);
        }
    }

    #[test]
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.open_access.is_some(), "paper {} missing open_access", p.id);
        }
    }

    #[test]
    fn parse_abstract_from_inverted_index() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // Result 1 has abstract_inverted_index
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "at least one paper should have abstract");
        for p in &with_abstract {
            let text = p.abstract_text.as_ref().unwrap();
            assert!(text.len() > 10, "abstract too short: {}", text);
        }
    }

    #[test]
    fn parse_empty_results() {
        let papers = parse_search_response(r#"{"results": []}"#).unwrap();
        assert!(papers.is_empty());
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
    fn search_request_path_contains_works() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/works".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_search_param() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("search=test".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_per_page() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("per_page=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn build_search_url_omits_mailto_without_email() {
        let url = build_search_url("https://api.openalex.org", &crate::sources::SearchQuery::simple("attention", 3), None, None).unwrap();
        assert!(
            !url.contains("mailto="),
            "url should carry no contact address: {}",
            url
        );
    }

    #[test]
    fn build_search_url_includes_mailto_with_email() {
        let url = build_search_url("https://api.openalex.org", &crate::sources::SearchQuery::simple("attention", 3), None, Some("a@b.com")).unwrap();
        assert!(url.contains("mailto=a@b.com"), "got: {}", url);
    }

    #[test]
    #[serial]
    fn search_uses_env_email_when_set() {
        unsafe { std::env::set_var("FASTPAPER_EMAIL", "custom@example.com") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("mailto=custom%40example.com|mailto=custom@example.com".to_string()))
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
        build_search_url("https://api.openalex.org", q, None, None).unwrap()
    }

    #[test]
    fn plain_query_uses_search_and_per_page() {
        let u = url(&SearchQuery::simple("attention", 10));
        assert!(u.contains("search=attention"), "got: {}", u);
        assert!(u.contains("per_page=10"), "got: {}", u);
    }

    // OpenAlex pages results; it has no offset parameter.
    #[test]
    fn offset_becomes_a_page_number() {
        let mut q = SearchQuery::simple("attention", 10);
        q.offset = 20;
        assert!(url(&q).contains("page=3"), "got: {}", url(&q));
    }

    #[test]
    fn zero_offset_is_page_one() {
        assert!(url(&SearchQuery::simple("attention", 10)).contains("page=1"));
    }

    #[test]
    fn offset_that_is_not_a_whole_page_is_rejected() {
        let mut q = SearchQuery::simple("attention", 10);
        q.offset = 15;
        let err = build_search_url("https://api.openalex.org", &q, None, None).unwrap_err();
        assert!(err.contains("multiple of"), "got: {}", err);
    }

    #[test]
    fn author_uses_the_raw_name_search_filter() {
        let mut q = SearchQuery::simple("attention", 10);
        q.author = Some("Vaswani".into());
        assert!(
            url(&q).contains("raw_author_name.search:Vaswani"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn year_uses_publication_year() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2017);
        assert!(url(&q).contains("publication_year:2017"), "got: {}", url(&q));
    }

    #[test]
    fn dates_use_from_and_to_publication_date() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2024-01-01".into());
        q.before = Some("2024-03-31".into());
        let u = url(&q);
        assert!(u.contains("from_publication_date:2024-01-01"), "got: {}", u);
        assert!(u.contains("to_publication_date:2024-03-31"), "got: {}", u);
    }

    #[test]
    fn open_access_uses_is_oa() {
        let mut q = SearchQuery::simple("attention", 10);
        q.open_access = true;
        assert!(url(&q).contains("is_oa:true"), "got: {}", url(&q));
    }

    #[test]
    fn field_passes_through_as_a_concept_id() {
        let mut q = SearchQuery::simple("attention", 10);
        q.field = Some("C154945302".into());
        assert!(url(&q).contains("concepts.id:C154945302"), "got: {}", url(&q));
    }

    #[test]
    fn filters_share_one_comma_separated_parameter() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2017);
        q.open_access = true;
        let u = url(&q);
        assert_eq!(u.matches("filter=").count(), 1, "one filter parameter: {}", u);
    }

    #[test]
    fn malformed_date_is_rejected() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2024".into());
        let err = build_search_url("https://api.openalex.org", &q, None, None).unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "got: {}", err);
    }

    #[test]
    fn sort_by_citations_uses_cited_by_count() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Citations);
        assert!(url(&q).contains("sort=cited_by_count:desc"), "got: {}", url(&q));
    }

    #[test]
    fn sort_by_date_uses_publication_date() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(url(&q).contains("sort=publication_date:asc"), "got: {}", url(&q));
    }

    #[test]
    fn sort_by_relevance_uses_relevance_score() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Relevance);
        assert!(url(&q).contains("sort=relevance_score:desc"), "got: {}", url(&q));
    }

    #[test]
    fn api_key_is_sent_when_configured() {
        let q = SearchQuery::simple("attention", 10);
        let u = build_search_url("https://api.openalex.org", &q, Some("k123"), None).unwrap();
        assert!(u.contains("api_key=k123"), "got: {}", u);
    }
}
