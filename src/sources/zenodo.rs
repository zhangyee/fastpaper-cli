use super::Paper;

/// Zenodo rejects `size` above 25 for anonymous callers with a bare HTTP 400,
/// so catch it here and say what the limit is.
const ANONYMOUS_MAX_SIZE: u32 = 25;

/// Build the records search URL.
///
/// Zenodo's `access_right` query parameter is ignored -- open and closed return
/// identical counts -- so the restriction goes into `q` as an Elasticsearch
/// term, which does filter. Author and dates work the same way.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    if q.limit > ANONYMOUS_MAX_SIZE {
        return Err(format!(
            "zenodo caps results at {} per request for anonymous callers (asked for {})",
            ANONYMOUS_MAX_SIZE, q.limit
        ));
    }
    if q.limit > 0 && q.offset % q.limit != 0 {
        return Err(format!(
            "zenodo pages results, so --offset must be a multiple of -n \
             (got --offset {} with -n {})",
            q.offset, q.limit
        ));
    }
    let page = if q.limit > 0 { q.offset / q.limit + 1 } else { 1 };

    let mut terms = vec![super::encode_query(&q.query)];
    if let Some(ref author) = q.author {
        terms.push(format!("creators.name:{}", super::encode_query(author)));
    }
    match q.year {
        Some(year) => terms.push(format!(
            "publication_date:%5B{}-01-01+TO+{}-12-31%5D",
            year, year
        )),
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
                terms.push(format!("publication_date:%5B{}+TO+{}%5D", from, to));
            }
        }
    }
    if q.open_access {
        terms.push("access_right:open".to_string());
    }

    let mut url = format!(
        "{}/api/records?q={}&size={}&page={}&type=publication",
        base_url,
        terms.join("+AND+"),
        q.limit,
        page
    );

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "bestmatch",
            super::SortField::Date => "mostrecent",
            super::SortField::Citations => {
                return Err(
                    "zenodo cannot sort by citations: it records no citation counts.\n\
                     Try semantic or openalex."
                        .to_string(),
                );
            }
        };
        url.push_str(&format!("&sort={}", by));
    }

    Ok(url)
}

/// Search Zenodo API.
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

/// Parse Zenodo JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let hits = root["hits"]["hits"]
        .as_array()
        .ok_or("missing 'hits.hits' array")?;

    let mut papers = Vec::new();
    for item in hits {
        let meta = &item["metadata"];

        let title = meta["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let authors: Vec<String> = meta["creators"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());

        let abstract_text = meta["description"].as_str().map(|s| s.to_string());

        let year = meta["publication_date"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let pdf_url = item["files"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|f| {
                        f["key"]
                            .as_str()
                            .map(|k| k.ends_with(".pdf"))
                            .unwrap_or(false)
                    })
                    .and_then(|f| f["links"]["self"].as_str().map(|s| s.to_string()))
            });

        let is_open = meta["access_right"].as_str() == Some("open");

        let id = item["id"].as_u64().map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["links"]["html"].as_str().map(|s| s.to_string()),
            pdf_url,
            venue: None,
            citations: None,
            fields: vec![],
            open_access: Some(is_open),
            source: "zenodo".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/zenodo_search.json");

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
    fn parse_source_is_zenodo() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "zenodo");
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
    fn parse_pdf_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
    }

    #[test]
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.open_access.is_some(), "paper {} missing open_access", p.id);
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
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/api/records".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_request_contains_size() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("size=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    // Zenodo caps `size` at 25 for anonymous callers; passing -n 50 straight
    // through yields an unexplained HTTP 400.
    #[test]
    fn search_limit_above_anonymous_cap_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(400)
            .create();
        let err = search(&server.url(), &crate::sources::SearchQuery::simple("test", 50)).unwrap_err();
        assert!(
            err.contains("25"),
            "error should state the cap of 25, got: {}",
            err
        );
    }

    #[test]
    fn search_at_anonymous_cap_is_allowed() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        assert!(search(&server.url(), &crate::sources::SearchQuery::simple("test", 25)).is_ok());
    }

    #[test]
    fn search_request_contains_type_publication() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("type=publication".to_string()))
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
    use crate::sources::{SearchQuery, SortField};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://zenodo.org", q).unwrap()
    }

    // Zenodo's access_right query parameter is ignored: open and closed both
    // return 2674 for the same search. The in-query term does filter.
    #[test]
    fn open_access_goes_into_the_query_not_a_parameter() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.open_access = true;
        let u = url(&q);
        assert!(u.contains("access_right:open"), "got: {}", u);
        assert!(!u.contains("&access_right="), "the parameter is ignored: {}", u);
    }

    #[test]
    fn author_becomes_a_creators_name_term() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Zhang".into());
        assert!(url(&q).contains("creators.name:Zhang"), "got: {}", url(&q));
    }

    #[test]
    fn year_becomes_a_publication_date_range() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        assert!(
            url(&q).contains("publication_date:%5B2024-01-01+TO+2024-12-31%5D"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut q = SearchQuery::simple("crispr", 5);
        q.offset = 10;
        assert!(url(&q).contains("page=3"), "got: {}", url(&q));
    }

    #[test]
    fn sort_by_date_uses_mostrecent() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Date);
        assert!(url(&q).contains("sort=mostrecent"));
    }

    #[test]
    fn sort_by_citations_is_rejected() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Citations);
        assert!(build_search_url("https://zenodo.org", &q).is_err());
    }

    #[test]
    fn limit_above_the_anonymous_cap_is_still_rejected() {
        let err = build_search_url("https://zenodo.org", &SearchQuery::simple("x", 50))
            .unwrap_err();
        assert!(err.contains("25"), "got: {}", err);
    }
}
