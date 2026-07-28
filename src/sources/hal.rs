use super::Paper;

const FIELDS: &str =
    "halId_s,title_s,authFullName_s,abstract_s,doiId_s,publicationDateY_i,fileMain_s,uri_s";

/// Build the Solr search URL.
///
/// HAL is a Solr index: `q` holds the query and author term, structured
/// restrictions go into repeated `fq` parameters, and `start`/`rows` page.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let mut terms = vec![super::encode_query(&q.query)];
    if let Some(ref author) = q.author {
        terms.push(format!(
            "authFullName_s:%22{}%22",
            super::encode_query(author)
        ));
    }

    let mut url = format!(
        "{}/search/?q={}&rows={}&start={}&wt=json&fl={}",
        base_url,
        terms.join("+AND+"),
        q.limit,
        q.offset,
        FIELDS
    );

    if let Some(year) = q.year {
        url.push_str(&format!("&fq=publicationDateY_i:{}", year));
    }
    if q.open_access {
        url.push_str("&fq=openAccess_bool:true");
    }
    // domain_s values are hierarchical codes like "0.sdv" / "1.sdv.bbm", so a
    // bare "sdv" only matches when wrapped in wildcards.
    if let Some(ref field) = q.field {
        url.push_str(&format!("&fq=domain_s:*{}*", super::encode_query(field)));
    }

    if let Some(sort) = q.sort {
        match sort {
            // Solr's default ordering is by relevance score.
            super::SortField::Relevance => {}
            super::SortField::Date => {
                let order = match q.order {
                    super::SortOrder::Asc => "asc",
                    super::SortOrder::Desc => "desc",
                };
                url.push_str(&format!("&sort=publicationDateY_i+{}", order));
            }
            super::SortField::Citations => {
                return Err(
                    "hal cannot sort by citations: it indexes no citation counts.\n\
                     Try semantic or openalex."
                        .to_string(),
                );
            }
        }
    }

    Ok(url)
}

/// Fetch a single document by HAL id or DOI.
///
/// HAL has no by-id path; a field-scoped Solr query is the documented way.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let field = if identifier.starts_with("10.") {
        "doiId_s"
    } else {
        "halId_s"
    };
    let url = format!(
        "{}/search/?q={}:%22{}%22&rows=1&wt=json&fl={}",
        base_url,
        field,
        super::encode_query(identifier),
        FIELDS
    );
    let body = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok(parse_search_response(&body)?.into_iter().next())
}

/// Search HAL API.
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

/// Parse HAL JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let docs = root["response"]["docs"]
        .as_array()
        .ok_or("missing 'response.docs' array")?;

    let mut papers = Vec::new();
    for item in docs {
        // title_s can be array or string
        let title = if let Some(arr) = item["title_s"].as_array() {
            arr.first()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            item["title_s"].as_str().unwrap_or("").to_string()
        };
        if title.is_empty() {
            continue;
        }

        let id = item["halId_s"].as_str().unwrap_or("").to_string();

        let authors: Vec<String> = item["authFullName_s"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doiId_s"].as_str().map(|s| s.to_string());

        // abstract_s can be array or string
        let abstract_text = if let Some(arr) = item["abstract_s"].as_array() {
            arr.first().and_then(|v| v.as_str()).map(|s| s.to_string())
        } else {
            item["abstract_s"].as_str().map(|s| s.to_string())
        };

        let year = item["publicationDateY_i"].as_u64().map(|y| y as u16);

        let pdf_url = item["fileMain_s"].as_str().map(|s| s.to_string());

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["uri_s"].as_str().map(|s| s.to_string()),
            pdf_url,
            venue: None,
            citations: None,
            fields: vec![],
            open_access: Some(true),
            source: "hal".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/hal_search.json");

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
    fn parse_source_is_hal() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "hal");
        }
    }

    #[test]
    fn parse_id_is_hal_format() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(
                p.id.starts_with("hal-"),
                "id should start with hal-: {}",
                p.id
            );
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
    fn parse_pdf_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
    }

    #[test]
    fn parse_open_access_always_true() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.open_access, Some(true), "HAL papers are always OA");
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
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/search/".to_string()))
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
    fn search_request_contains_wt_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("wt=json".to_string()))
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
    fn search_request_contains_rows() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("rows=3".to_string()))
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
    fn search_request_contains_fl() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("fl=".to_string()))
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
    use crate::sources::{SearchQuery, SortField, SortOrder};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://api.archives-ouvertes.fr", q).unwrap()
    }

    #[test]
    fn author_becomes_a_solr_term_on_the_query() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Aubry".into());
        assert!(
            url(&q).contains("authFullName_s:%22Aubry%22"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn year_becomes_a_filter_query() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        assert!(
            url(&q).contains("fq=publicationDateY_i:2024"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn open_access_becomes_a_filter_query() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.open_access = true;
        assert!(
            url(&q).contains("fq=openAccess_bool:true"),
            "got: {}",
            url(&q)
        );
    }

    // domain_s holds hierarchical codes ("0.sdv", "1.sdv.bbm"), so a plain
    // "sdv" only matches wrapped in wildcards.
    #[test]
    fn field_is_wrapped_in_wildcards() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.field = Some("sdv".into());
        assert!(url(&q).contains("fq=domain_s:*sdv*"), "got: {}", url(&q));
    }

    #[test]
    fn offset_becomes_start() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 30;
        assert!(url(&q).contains("start=30"));
    }

    #[test]
    fn sort_by_date_uses_the_publication_year_field() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(
            url(&q).contains("sort=publicationDateY_i+asc"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn sort_by_citations_is_rejected() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Citations);
        assert!(build_search_url("https://api.archives-ouvertes.fr", &q).is_err());
    }
}

#[cfg(test)]
mod get_tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/hal_search.json");

    #[test]
    fn get_by_id_queries_hal_id_for_a_hal_identifier() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", mockito::Matcher::Regex("halId_s".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = get_by_id(&server.url(), "hal-00000001");
        m.assert();
    }

    // HAL has no by-id path, so a DOI has to target the DOI field instead.
    #[test]
    fn get_by_id_queries_doi_id_for_a_doi() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", mockito::Matcher::Regex("doiId_s".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = get_by_id(&server.url(), "10.1038/nature12373");
        m.assert();
    }

    #[test]
    fn get_by_id_returns_the_first_document() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        assert!(get_by_id(&server.url(), "hal-00000001").unwrap().is_some());
    }
}
