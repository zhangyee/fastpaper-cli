use super::Paper;

/// Wrap a filter value in quotes when it contains characters the API refuses
/// bare -- it rejects unquoted values with spaces, parentheses or operators.
fn quoted(value: &str) -> String {
    if value.contains(' ') || value.contains('(') || value.contains(')') {
        format!("%22{}%22", super::encode_query(value).replace('+', "%20"))
    } else {
        super::encode_query(value)
    }
}

/// Build the Graph API search URL.
///
/// This is the OpenAIRE Graph API v3; the old /search/researchProducts endpoint
/// returns a different, legacy envelope. Filters are separate parameters and
/// the API self-documents: an unknown one is answered with the full valid list.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    if q.limit > 0 && q.offset % q.limit != 0 {
        return Err(format!(
            "openaire pages results, so --offset must be a multiple of -n \
             (got --offset {} with -n {})",
            q.offset, q.limit
        ));
    }
    let page = if q.limit > 0 { q.offset / q.limit + 1 } else { 1 };

    let mut url = format!(
        "{}/graph/v3/research-products?search={}&pageSize={}&page={}&type=publication",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        page
    );

    if let Some(ref author) = q.author {
        url.push_str(&format!("&authorFullName={}", quoted(author)));
    }
    match q.year {
        Some(year) => url.push_str(&format!("&publicationYear={}", year)),
        None => {
            if let Some(ref after) = q.after {
                url.push_str(&format!("&fromPublicationDate={}", super::validate_ymd(after)?));
            }
            if let Some(ref before) = q.before {
                url.push_str(&format!("&toPublicationDate={}", super::validate_ymd(before)?));
            }
        }
    }
    if q.open_access {
        url.push_str("&accessRightLabel=%22Open%20Access%22");
    }
    // `fos` takes a value from OpenAIRE's field-of-science vocabulary; an
    // unrecognised one comes back with the allowed list, which is more useful
    // than anything we could say here.
    if let Some(ref field) = q.field {
        url.push_str(&format!("&fos={}", quoted(field)));
    }

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "relevance",
            super::SortField::Date => "publicationDate",
            super::SortField::Citations => "citationCount",
        };
        let order = match q.order {
            super::SortOrder::Asc => "ASC",
            super::SortOrder::Desc => "DESC",
        };
        url.push_str(&format!("&sortBy={}%20{}", by, order));
    }

    Ok(url)
}

/// Fetch a single research product by its OpenAIRE id.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/graph/v3/research-products?id={}",
        base_url,
        super::encode_query(identifier)
    );
    let body = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok(parse_search_response(&body)?.into_iter().next())
}

/// Search the OpenAIRE Graph API.
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

/// Parse an OpenAIRE Graph v3 response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array")?;

    let mut papers = Vec::new();
    for item in results {
        let title = item["mainTitle"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["fullName"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Identifiers and links live on the instances, not the record.
        let instances = item["instances"].as_array();
        let doi = instances.and_then(|arr| {
            arr.iter()
                .filter_map(|i| i["pids"].as_array())
                .flatten()
                .find(|p| p["scheme"].as_str() == Some("doi"))
                .and_then(|p| p["value"].as_str().map(|s| s.to_string()))
        });
        let url = instances.and_then(|arr| {
            arr.iter()
                .filter_map(|i| i["urls"].as_array())
                .flatten()
                .find_map(|u| u.as_str().map(|s| s.to_string()))
        });

        let year = item["publicationDate"]
            .as_str()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let abstract_text = item["descriptions"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());

        let fields: Vec<String> = item["subjects"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(5)
                    .filter_map(|s| s["subject"]["value"].as_str().map(|v| v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let citations = item["indicators"]["citationImpact"]["citationCount"]
            .as_f64()
            .map(|n| n as u32);

        let open_access = item["bestAccessRight"]["label"]
            .as_str()
            .map(|l| l.eq_ignore_ascii_case("OPEN") || l.eq_ignore_ascii_case("Open Access"));

        papers.push(Paper {
            id: item["id"].as_str().unwrap_or("").to_string(),
            title,
            authors,
            abstract_text,
            year,
            doi,
            url,
            pdf_url: None,
            venue: item["container"]["name"].as_str().map(|s| s.to_string()),
            citations,
            fields,
            open_access,
            source: "openaire".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openaire_graph_v3.json");

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
    fn parse_source_is_openaire() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "openaire");
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
    fn parse_authors_from_creator() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi_from_pid() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_abstract_from_description() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_year_from_dateofacceptance() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() >= 2000);
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




}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField, SortOrder};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://api.openaire.eu", q).unwrap()
    }

    // The legacy /search/researchProducts endpoint returns a different envelope.
    #[test]
    fn uses_the_graph_v3_endpoint() {
        let u = url(&SearchQuery::simple("crispr", 10));
        assert!(u.contains("/graph/v3/research-products?"), "got: {}", u);
        assert!(!u.contains("researchProducts?keywords"), "legacy shape: {}", u);
    }

    #[test]
    fn author_uses_author_full_name() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.author = Some("Doudna".into());
        assert!(url(&q).contains("authorFullName=Doudna"), "got: {}", url(&q));
    }

    #[test]
    fn year_uses_publication_year() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.year = Some(2024);
        assert!(url(&q).contains("publicationYear=2024"), "got: {}", url(&q));
    }

    #[test]
    fn dates_use_from_and_to_publication_date() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.after = Some("2024-01-01".into());
        q.before = Some("2024-03-31".into());
        let u = url(&q);
        assert!(u.contains("fromPublicationDate=2024-01-01"), "got: {}", u);
        assert!(u.contains("toPublicationDate=2024-03-31"), "got: {}", u);
    }

    // The API refuses unquoted values containing spaces.
    #[test]
    fn open_access_value_is_quoted() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.open_access = true;
        assert!(
            url(&q).contains("accessRightLabel=%22Open%20Access%22"),
            "got: {}",
            url(&q)
        );
    }

    #[test]
    fn multi_word_field_values_are_quoted() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.field = Some("03 medical and health sciences".into());
        let u = url(&q);
        assert!(u.contains("fos=%22"), "should be quoted: {}", u);
        assert!(u.contains("%20"), "spaces encoded, not '+': {}", u);
    }

    #[test]
    fn single_word_field_values_are_not_quoted() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.field = Some("biology".into());
        assert!(url(&q).contains("fos=biology"), "got: {}", url(&q));
    }

    #[test]
    fn sort_uses_field_then_direction() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.sort = Some(SortField::Citations);
        assert!(url(&q).contains("sortBy=citationCount%20DESC"), "got: {}", url(&q));
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(url(&q).contains("sortBy=publicationDate%20ASC"), "got: {}", url(&q));
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut q = SearchQuery::simple("crispr", 10);
        q.offset = 20;
        assert!(url(&q).contains("page=3"), "got: {}", url(&q));
    }
}

#[cfg(test)]
mod get_tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openaire_graph_v3.json");

    #[test]
    fn get_by_id_uses_the_id_parameter() {
        let mut server = mockito::Server::new();
        let m = server.mock("GET", mockito::Matcher::Regex("id=openaire".to_string()))
            .with_status(200).with_body(FIXTURE).create();
        let _ = get_by_id(&server.url(), "openaire____::abc");
        m.assert();
    }

    #[test]
    fn get_by_id_returns_the_first_result() {
        let mut server = mockito::Server::new();
        server.mock("GET", mockito::Matcher::Any).with_status(200).with_body(FIXTURE).create();
        assert!(get_by_id(&server.url(), "openaire____::abc").unwrap().is_some());
    }
}
