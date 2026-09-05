//! DataCite — the DOI registry for research output that Crossref does not
//! cover: datasets, software, and a very large body of university theses and
//! institutional-repository deposits.
//!
//! It is the natural complement to the `crossref` source: between them they
//! cover most of the registered DOI space.
//!
//! One trap is baked into the URL builder. DataCite advertises a
//! `publication-year` query parameter, but it is **silently ignored** — asking
//! for 2020 returns a mix of years, with no error. The year has to go into the
//! Lucene query as `publicationYear:N` instead, which does filter.

use super::Paper;

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let mut query = super::encode_query(&q.query);

    // Not `&publication-year=`: that parameter is accepted and ignored.
    if let Some(year) = q.year {
        query.push_str(&format!("+AND+publicationYear%3A{}", year));
    }

    let page = page_for(q)?;
    Ok(format!(
        "{}/dois?query={}&page%5Bsize%5D={}&page%5Bnumber%5D={}",
        base_url, query, q.limit, page
    ))
}

fn page_for(q: &super::SearchQuery) -> Result<u32, String> {
    if q.offset == 0 {
        return Ok(1);
    }
    if q.limit == 0 || !q.offset.is_multiple_of(q.limit) {
        return Err(format!(
            "datacite pages by page number, so --offset must be a multiple of -n ({}): got {}.",
            q.limit, q.offset
        ));
    }
    Ok(q.offset / q.limit + 1)
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    parse_search_response(&http_get(&build_search_url(base_url, q)?)?)
}

/// Fetch one record by DOI.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/dois/{}",
        base_url,
        super::encode_query(identifier).replace('+', "%20")
    );
    let body = match http_get(&url) {
        Ok(b) => b,
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let root: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(parse_record(&root["data"]))
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
                last_err = "datacite rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => return Err("datacite returned 404".to_string()),
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("datacite server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("datacite returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(errors) = root["errors"].as_array()
        && let Some(first) = errors.first()
    {
        let detail = first["title"]
            .as_str()
            .or_else(|| first["detail"].as_str())
            .unwrap_or("unknown error");
        return Err(format!("datacite API error: {}", detail));
    }

    let data = root["data"]
        .as_array()
        .ok_or("missing 'data' array in DataCite response")?;

    Ok(data.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    let a = &item["attributes"];
    let title = a["titles"][0]["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }

    // Personal names come as `name`; organisational creators use the same key.
    let authors = a["creators"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // `descriptions` holds several types; the abstract is the useful one, but
    // fall back to whatever is first rather than dropping the text entirely.
    let abstract_text = a["descriptions"]
        .as_array()
        .and_then(|d| {
            d.iter()
                .find(|x| x["descriptionType"].as_str() == Some("Abstract"))
                .or_else(|| d.first())
        })
        .and_then(|d| d["description"].as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let doi = a["doi"].as_str().map(|s| s.to_string());

    Some(Paper {
        id: doi.clone().unwrap_or_else(|| {
            item["id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_default()
        }),
        title: title.to_string(),
        authors,
        abstract_text,
        year: a["publicationYear"]
            .as_u64()
            .map(|y| y as u16)
            .or_else(|| a["publicationYear"].as_str()?.parse().ok()),
        doi,
        url: a["url"].as_str().map(|s| s.to_string()),
        // DataCite registers metadata; the file lives with the repository and
        // the record does not say whether it is free to read.
        pdf_url: None,
        venue: a["publisher"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| a["publisher"]["name"].as_str().map(|s| s.to_string())),
        citations: None,
        fields: a["types"]["resourceTypeGeneral"]
            .as_str()
            .map(|t| vec![t.to_string()])
            .unwrap_or_default(),
        open_access: None,
        source: "datacite".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/datacite_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    #[test]
    fn parses_every_record_in_the_fixture() {
        assert_eq!(parse_search_response(FIXTURE).unwrap().len(), 3);
    }

    #[test]
    fn sets_the_source_name() {
        assert!(
            parse_search_response(FIXTURE)
                .unwrap()
                .iter()
                .all(|p| p.source == "datacite")
        );
    }

    #[test]
    fn uses_the_doi_as_the_id() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].id, papers[0].doi.clone().unwrap());
        assert!(papers[0].id.starts_with("10."), "got: {}", papers[0].id);
    }

    #[test]
    fn reads_title_creators_and_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers[0].title.is_empty());
        assert!(papers.iter().any(|p| !p.authors.is_empty()));
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    #[test]
    fn prefers_the_abstract_over_other_description_types() {
        let json = r#"{"data":[{"id":"10.1/x","attributes":{"titles":[{"title":"T"}],
            "descriptions":[{"description":"methods text","descriptionType":"Methods"},
                            {"description":"the abstract","descriptionType":"Abstract"}]}}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].abstract_text.as_deref(), Some("the abstract"));
    }

    #[test]
    fn falls_back_to_the_first_description_when_none_is_an_abstract() {
        let json = r#"{"data":[{"id":"10.1/x","attributes":{"titles":[{"title":"T"}],
            "descriptions":[{"description":"only text","descriptionType":"Other"}]}}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].abstract_text.as_deref(), Some("only text"));
    }

    #[test]
    fn carries_the_resource_type_as_a_field() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| !p.fields.is_empty()));
    }

    // DataCite is a registry: it knows of no file and no access status.
    #[test]
    fn reports_no_pdf_no_citations_and_no_access_status() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.pdf_url.is_none()));
        assert!(papers.iter().all(|p| p.citations.is_none()));
        assert!(papers.iter().all(|p| p.open_access.is_none()));
    }

    #[test]
    fn an_empty_data_array_is_no_papers() {
        assert!(parse_search_response(r#"{"data":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn a_record_with_no_title_is_skipped() {
        let json = r#"{"data":[{"id":"10.1/x","attributes":{"titles":[]}}]}"#;
        assert!(parse_search_response(json).unwrap().is_empty());
    }

    #[test]
    fn nulls_and_absent_keys_do_not_fail_parsing() {
        let json = r#"{"data":[{"id":"10.1/x","attributes":{"titles":[{"title":"T"}],
                      "creators":null,"descriptions":null,"publicationYear":null}}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].authors.is_empty());
        assert!(papers[0].year.is_none());
    }

    #[test]
    fn an_error_body_becomes_an_err() {
        let json = r#"{"errors":[{"title":"Bad request"}]}"#;
        let err = parse_search_response(json).unwrap_err();
        assert!(err.contains("Bad request"), "got: {}", err);
    }

    #[test]
    fn a_body_with_no_data_array_is_an_error() {
        assert!(parse_search_response(r#"{"meta":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    #[test]
    fn search_url_carries_query_and_page_size() {
        let url = build_search_url("https://api.datacite.org", &q("climate")).unwrap();
        assert!(url.contains("query=climate"), "got: {}", url);
        assert!(url.contains("page%5Bsize%5D=10"), "got: {}", url);
    }

    // The `publication-year` parameter is accepted and ignored by DataCite, so
    // the year has to ride in the Lucene query or it silently does nothing.
    #[test]
    fn year_goes_into_the_query_not_the_ignored_parameter() {
        let mut query = q("climate");
        query.year = Some(2020);
        let url = build_search_url("https://api.datacite.org", &query).unwrap();
        assert!(url.contains("publicationYear%3A2020"), "got: {}", url);
        assert!(!url.contains("publication-year="), "got: {}", url);
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut query = q("climate");
        query.offset = 20;
        assert!(
            build_search_url("https://api.datacite.org", &query)
                .unwrap()
                .contains("page%5Bnumber%5D=3")
        );
    }

    #[test]
    fn an_offset_off_the_page_boundary_is_rejected() {
        let mut query = q("climate");
        query.offset = 5;
        assert!(build_search_url("https://api.datacite.org", &query).is_err());
    }
}
