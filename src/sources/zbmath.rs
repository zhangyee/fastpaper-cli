//! zbMATH Open — the mathematics abstracting and reviewing service, open to
//! everyone since 2021.
//!
//! What it adds over arXiv is the *published* mathematical record: journal
//! articles back to the 19th century, each with an MSC classification and,
//! often, a signed review. It publishes no full text and no citation counts,
//! so `Paper` comes back deliberately sparse.

use super::Paper;

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let mut terms = vec![super::encode_query(&q.query)];

    // zbMATH scopes a term with a field prefix inside the search string; it has
    // no separate author parameter.
    if let Some(ref author) = q.author {
        terms.push(format!("au%3A{}", super::encode_query(author)));
    }

    let page = page_for(q)?;
    Ok(format!(
        "{}/v1/document/_search?search_string={}&results_per_page={}&page={}",
        base_url,
        terms.join("+%26+"),
        q.limit,
        page
    ))
}

/// zbMATH pages by number, so an offset only lands on a page boundary.
fn page_for(q: &super::SearchQuery) -> Result<u32, String> {
    if q.offset == 0 {
        return Ok(1);
    }
    if q.limit == 0 || !q.offset.is_multiple_of(q.limit) {
        return Err(format!(
            "zbmath pages by page number, so --offset must be a multiple of -n ({}): got {}.",
            q.limit, q.offset
        ));
    }
    Ok(q.offset / q.limit + 1)
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;
    parse_search_response(&http_get(&url)?)
}

/// Fetch one document by its zbMATH id.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/v1/document/{}",
        base_url,
        super::encode_query(identifier)
    );
    let body = match http_get(&url) {
        Ok(b) => b,
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let root: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {}", e))?;
    // The by-id form returns a single object under `result`.
    Ok(parse_record(&root["result"]))
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
                last_err = "zbmath rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => return Err("zbmath returned 404".to_string()),
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("zbmath server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("zbmath returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    // zbMATH reports failure in a status block while still answering 200.
    if let Some(ok) = root["status"]["execution_bool"].as_bool()
        && !ok
    {
        let msg = root["status"]["execution"]
            .as_str()
            .unwrap_or("unknown error");
        return Err(format!("zbmath API error: {}", msg));
    }

    let results = root["result"]
        .as_array()
        .ok_or("missing 'result' array in zbMATH response")?;

    Ok(results.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    // The title is nested one level deeper than the field name suggests.
    let title = item["title"]["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }

    let id = item["id"]
        .as_u64()
        .map(|n| n.to_string())
        .or_else(|| item["id"].as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let authors = item["contributors"]["authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // `year` comes back as a string.
    let year = item["year"]
        .as_str()
        .and_then(|y| y.parse().ok())
        .or_else(|| item["year"].as_u64().map(|y| y as u16));

    // DOIs live in the typed `links` array, not in a field of their own.
    let doi = item["links"].as_array().and_then(|links| {
        links
            .iter()
            .find(|l| l["type"].as_str() == Some("doi"))
            .and_then(|l| l["identifier"].as_str())
            .map(|s| s.to_string())
    });

    let fields = item["msc"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["code"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(Paper {
        id,
        title: title.to_string(),
        authors,
        // zbMATH publishes reviews, not author abstracts, and the search
        // response carries neither.
        abstract_text: None,
        year,
        doi,
        url: item["zbmath_url"].as_str().map(|s| s.to_string()),
        pdf_url: None,
        venue: item["source"]["series"][0]["title"]
            .as_str()
            .map(|s| s.to_string()),
        citations: None,
        fields,
        open_access: None,
        source: "zbmath".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/zbmath_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    #[test]
    fn parses_every_result_in_the_fixture() {
        assert_eq!(parse_search_response(FIXTURE).unwrap().len(), 3);
    }

    #[test]
    fn sets_the_source_name() {
        assert!(
            parse_search_response(FIXTURE)
                .unwrap()
                .iter()
                .all(|p| p.source == "zbmath")
        );
    }

    // The field called `title` is an object; the string is one level down.
    #[test]
    fn reads_the_nested_title() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers[0].title.is_empty());
        assert!(
            !papers[0].title.starts_with('{'),
            "got: {}",
            papers[0].title
        );
    }

    #[test]
    fn reads_authors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| !p.authors.is_empty()));
    }

    // `year` is a JSON string here, not a number.
    #[test]
    fn parses_the_year_from_its_string_form() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    #[test]
    fn picks_the_doi_out_of_the_typed_links_array() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let doi = papers
            .iter()
            .find_map(|p| p.doi.clone())
            .expect("no DOI parsed");
        assert!(doi.starts_with("10."), "got: {}", doi);
    }

    #[test]
    fn uses_the_series_title_as_the_venue() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.venue.is_some()));
    }

    #[test]
    fn carries_msc_codes_as_fields() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| !p.fields.is_empty()));
    }

    // Everything zbMATH cannot say has to come back as None, not as a zero.
    #[test]
    fn reports_no_files_no_citations_and_no_access_status() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.pdf_url.is_none()));
        assert!(papers.iter().all(|p| p.citations.is_none()));
        assert!(papers.iter().all(|p| p.open_access.is_none()));
    }

    #[test]
    fn an_empty_result_array_is_no_papers() {
        assert!(
            parse_search_response(r#"{"result":[],"status":{"execution_bool":true}}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_failed_status_block_becomes_an_err() {
        let json = r#"{"result":[],"status":{"execution_bool":false,"execution":"bad query"}}"#;
        let err = parse_search_response(json).unwrap_err();
        assert!(err.contains("bad query"), "got: {}", err);
    }

    #[test]
    fn a_record_with_no_title_is_skipped() {
        let json = r#"{"result":[{"id":1,"title":{"title":""}}]}"#;
        assert!(parse_search_response(json).unwrap().is_empty());
    }

    #[test]
    fn nulls_and_absent_keys_do_not_fail_parsing() {
        let json =
            r#"{"result":[{"id":1,"title":{"title":"T"},"year":null,"links":null,"msc":null}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].year.is_none());
        assert!(papers[0].doi.is_none());
        assert!(papers[0].fields.is_empty());
    }

    #[test]
    fn a_body_with_no_result_array_is_an_error() {
        assert!(parse_search_response(r#"{"status":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    #[test]
    fn search_url_carries_query_and_page_size() {
        let url = build_search_url("https://api.zbmath.org", &q("manifold")).unwrap();
        assert!(url.contains("search_string=manifold"), "got: {}", url);
        assert!(url.contains("results_per_page=10"), "got: {}", url);
        assert!(url.contains("page=1"), "got: {}", url);
    }

    #[test]
    fn author_becomes_an_au_prefixed_term() {
        let mut query = q("manifold");
        query.author = Some("Yano".into());
        let url = build_search_url("https://api.zbmath.org", &query).unwrap();
        assert!(url.contains("au%3AYano"), "got: {}", url);
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut query = q("manifold");
        query.offset = 20;
        assert!(
            build_search_url("https://api.zbmath.org", &query)
                .unwrap()
                .contains("page=3")
        );
    }

    #[test]
    fn an_offset_off_the_page_boundary_is_rejected() {
        let mut query = q("manifold");
        query.offset = 5;
        assert!(build_search_url("https://api.zbmath.org", &query).is_err());
    }
}
