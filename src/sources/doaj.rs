use super::Paper;

/// Build the article search URL.
///
/// DOAJ takes the query as a path segment with Elasticsearch field syntax, so
/// author and year become `bibjson.*` terms ANDed onto it.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    if q.limit > 0 && q.offset % q.limit != 0 {
        return Err(format!(
            "doaj pages results, so --offset must be a multiple of -n \
             (got --offset {} with -n {})",
            q.offset, q.limit
        ));
    }
    let page = if q.limit > 0 {
        q.offset / q.limit + 1
    } else {
        1
    };

    let mut terms = vec![super::encode_query(&q.query)];
    if let Some(ref author) = q.author {
        terms.push(format!(
            "bibjson.author.name:{}",
            super::encode_query(author)
        ));
    }
    if let Some(year) = q.year {
        terms.push(format!("bibjson.year:{}", year));
    }
    // Every DOAJ article is open access by definition, so --open-access is
    // already satisfied and adds no term.

    // The query is a path segment, not a query-string value, so a space has to
    // be %20: '+' stays a literal plus there and silently breaks the boolean.
    // encode_query only ever emits '+' for spaces (a literal '+' becomes %2B),
    // so this rewrite is safe.
    let path = terms.join("%20AND%20").replace('+', "%20");

    let url = format!(
        "{}/api/v4/search/articles/{}?pageSize={}&page={}",
        base_url, path, q.limit, page
    );
    Ok(url)
}

/// Fetch a single article by its DOAJ id.
pub fn get_by_id(base_url: &str, id: &str) -> Result<Option<Paper>, String> {
    let url = format!("{}/api/v4/articles/{}", base_url, super::encode_query(id));
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
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Search DOAJ API.
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

/// Parse DOAJ JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array")?;

    let mut papers = Vec::new();
    for item in results {
        let bib = &item["bibjson"];

        let title = bib["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let authors: Vec<String> = bib["author"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = bib["identifier"].as_array().and_then(|arr| {
            arr.iter()
                .find(|id| id["type"].as_str() == Some("doi"))
                .and_then(|id| id["id"].as_str().map(|s| s.to_string()))
        });

        let abstract_text = bib["abstract"].as_str().map(|s| s.to_string());

        let year = bib["year"].as_str().and_then(|s| s.parse::<u16>().ok());

        let pdf_url = bib["link"].as_array().and_then(|arr| {
            arr.iter()
                .find(|l| {
                    l["type"]
                        .as_str()
                        .map(|t| t.contains("fulltext"))
                        .unwrap_or(false)
                })
                .and_then(|l| l["url"].as_str().map(|s| s.to_string()))
        });

        let id = item["id"].as_str().unwrap_or("").to_string();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: pdf_url.clone(),
            pdf_url,
            venue: bib["journal"]["title"].as_str().map(|s| s.to_string()),
            citations: None,
            fields: vec![],
            open_access: Some(true),
            source: "doaj".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/doaj_search.json");

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
    fn parse_source_is_doaj() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "doaj");
        }
    }

    #[test]
    fn parse_title() {
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
    fn parse_open_access_always_true() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.open_access, Some(true), "DOAJ papers are always OA");
        }
    }

    #[test]
    fn parse_pdf_url_from_fulltext_link() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
        for p in &with_pdf {
            assert!(p.pdf_url.as_ref().unwrap().starts_with("http"));
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
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_request_path_contains_search_articles() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex("/api/v4/search/articles/".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
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
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        mock.assert();
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::SearchQuery;

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://doaj.org", q).unwrap()
    }

    #[test]
    fn uses_the_versioned_api_path() {
        assert!(
            url(&SearchQuery::simple("crispr", 10)).contains("/api/v4/search/articles/"),
            "got: {}",
            url(&SearchQuery::simple("crispr", 10))
        );
    }

    #[test]
    fn author_becomes_a_bibjson_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Doudna".into());
        assert!(
            url(&q).contains("bibjson.author.name:Doudna"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn year_becomes_a_bibjson_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        assert!(url(&q).contains("bibjson.year:2024"), "got: {}", url(&q));
    }

    // DOAJ carries the query in a path segment, where '+' is a literal plus
    // rather than a space. `crispr+AND+bibjson.year:2024` matches nothing;
    // the %20 form returns thousands of hits.
    #[test]
    fn terms_are_joined_with_encoded_spaces_not_plus() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        let u = url(&q);
        assert!(u.contains("%20AND%20"), "got: {}", u);
        assert!(
            !u.contains("+AND+"),
            "path segments do not decode '+': {}",
            u
        );
    }

    #[test]
    fn spaces_inside_the_query_are_encoded_for_a_path_segment() {
        let u = url(&SearchQuery::simple("gene editing", 10));
        assert!(u.contains("gene%20editing"), "got: {}", u);
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
        q.offset = 5;
        let err = build_search_url("https://doaj.org", &q).unwrap_err();
        assert!(err.contains("multiple of"), "got: {}", err);
    }

    #[test]
    fn open_access_adds_no_term_because_doaj_is_all_open_access() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.open_access = true;
        assert_eq!(url(&q), url(&SearchQuery::simple("crispr", 10)));
    }
}
