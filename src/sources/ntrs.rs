//! NASA NTRS — the NASA Technical Reports Server.
//!
//! Aerospace engineering reports, conference papers and contractor studies,
//! most of them US government works and freely downloadable.
//!
//! Two shapes matter. Authors are not in an `authors` field but nested under
//! `authorAffiliations[].meta.author.name`; and the download links are
//! site-relative paths, so they have to be joined onto a host before use.

use super::Paper;

/// NTRS ignores every page-size parameter it is given -- `size`, `limit`, a
/// JSON `page` blob -- and always answers with exactly `PAGE` records. So the
/// page size is not ours to choose; only the offset is.
const PAGE: u32 = 10;

fn build_search_url(base_url: &str, q: &super::SearchQuery, from: u32) -> String {
    format!(
        "{}/api/citations/search?q={}&from={}",
        base_url,
        super::encode_query(&q.query),
        from
    )
}

/// Walk fixed-size pages until `-n` is satisfied.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let mut out: Vec<Paper> = Vec::new();
    let mut from = q.offset;
    while (out.len() as u32) < q.limit {
        let page = parse_search_response(&http_get(&build_search_url(base_url, q, from))?)?;
        if page.is_empty() {
            break;
        }
        let before = out.len();
        out.extend(page);
        // A short or repeated page means the result set is exhausted.
        if (out.len() - before) < PAGE as usize {
            break;
        }
        from += PAGE;
    }
    out.truncate(q.limit as usize);
    Ok(out)
}

/// Parse the single-record `/api/citations/<id>` body as a one-item list, so
/// the download resolver can reuse the same field mapping as search.
pub fn parse_citation_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(parse_record(&root).into_iter().collect())
}

/// Fetch one citation by its NTRS numeric id.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/api/citations/{}",
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
    Ok(parse_record(&root))
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
                last_err = "ntrs rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => return Err("ntrs returned 404".to_string()),
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("ntrs server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("ntrs returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(msg) = root["message"].as_str()
        && root["results"].is_null()
    {
        return Err(format!("ntrs API error: {}", msg));
    }

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array in NTRS response")?;

    Ok(results.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    let title = item["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }
    let id = item["id"]
        .as_u64()
        .map(|n| n.to_string())
        .or_else(|| item["id"].as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // Authors sit two levels down inside the affiliation entries.
    let authors = item["authorAffiliations"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["meta"]["author"]["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // The links are site-relative ("/api/citations/.../x.pdf").
    let pdf_url = item["downloads"].as_array().and_then(|d| {
        d.iter()
            .find_map(|entry| entry["links"]["pdf"].as_str())
            .map(|path| format!("https://ntrs.nasa.gov{}", path))
    });

    let fields = item["subjectCategories"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(Paper {
        id: id.clone(),
        title: title.to_string(),
        authors,
        abstract_text: item["abstract"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        year: item["distributionDate"]
            .as_str()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok()),
        // NTRS report numbers are not DOIs, and pretending otherwise would put
        // a non-resolvable string in the DOI field.
        doi: None,
        url: Some(format!("https://ntrs.nasa.gov/citations/{}", id)),
        open_access: Some(pdf_url.is_some()),
        pdf_url,
        venue: item["publications"][0]["publicationName"]
            .as_str()
            .map(|s| s.to_string()),
        citations: None,
        fields,
        source: "ntrs".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/ntrs_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    // The fixture holds a full page: NTRS always returns ten, whatever the
    // request asks for.
    #[test]
    fn parses_a_whole_fixed_size_page() {
        assert_eq!(parse_search_response(FIXTURE).unwrap().len(), PAGE as usize);
    }

    #[test]
    fn sets_the_source_name() {
        assert!(
            parse_search_response(FIXTURE)
                .unwrap()
                .iter()
                .all(|p| p.source == "ntrs")
        );
    }

    #[test]
    fn reads_id_and_title() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers[0].id.is_empty());
        assert!(!papers[0].title.is_empty());
    }

    // There is no `authors` key at all; the names are nested under the
    // affiliation entries, which is easy to miss and yields silent blanks.
    #[test]
    fn reads_authors_from_the_nested_affiliation_entries() {
        let json = r#"{"results":[{"id":1,"title":"T","authorAffiliations":[
            {"meta":{"author":{"name":"Liepack, Otfrid G."}}}]}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].authors, vec!["Liepack, Otfrid G."]);
    }

    // The API gives a path, not a URL; a bare path would not be fetchable.
    #[test]
    fn joins_the_relative_download_path_onto_the_host() {
        let json = r#"{"results":[{"id":1,"title":"T","downloads":[
            {"links":{"pdf":"/api/citations/1/downloads/1.pdf"}}]}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(
            papers[0].pdf_url.as_deref(),
            Some("https://ntrs.nasa.gov/api/citations/1/downloads/1.pdf")
        );
    }

    #[test]
    fn a_record_with_no_downloads_is_not_claimed_as_open_access() {
        let json = r#"{"results":[{"id":1,"title":"T","downloads":[]}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert!(papers[0].pdf_url.is_none());
        assert_eq!(papers[0].open_access, Some(false));
    }

    #[test]
    fn reads_the_year_from_the_distribution_date() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    // NTRS report numbers look like identifiers but do not resolve as DOIs.
    #[test]
    fn never_reports_a_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.doi.is_none()));
    }

    #[test]
    fn builds_the_citation_url_from_the_id() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(
            papers[0]
                .url
                .as_deref()
                .unwrap()
                .starts_with("https://ntrs.nasa.gov/citations/"),
            "got: {:?}",
            papers[0].url
        );
    }

    #[test]
    fn an_empty_result_list_is_no_papers() {
        assert!(
            parse_search_response(r#"{"results":[]}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_record_with_no_title_is_skipped() {
        assert!(
            parse_search_response(r#"{"results":[{"id":1,"title":""}]}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn nulls_and_absent_keys_do_not_fail_parsing() {
        let json = r#"{"results":[{"id":1,"title":"T","abstract":null,
                      "authorAffiliations":null,"downloads":null}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].abstract_text.is_none());
        assert!(papers[0].authors.is_empty());
    }

    #[test]
    fn a_body_with_no_results_is_an_error() {
        assert!(parse_search_response(r#"{"stats":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    // NTRS takes a true record offset, so any `--offset` is valid.
    #[test]
    fn offset_passes_through_as_a_record_offset() {
        let query = q("mars");
        let url = build_search_url("https://ntrs.nasa.gov", &query, 7);
        assert!(url.contains("from=7"), "got: {}", url);
    }

    // Asking for a page size is pointless here, so the URL must not pretend to.
    #[test]
    fn the_url_sends_no_page_size_because_ntrs_ignores_one() {
        let url = build_search_url("https://ntrs.nasa.gov", &q("mars"), 0);
        assert!(!url.contains("size="), "got: {}", url);
        assert!(!url.contains("limit="), "got: {}", url);
    }
}
