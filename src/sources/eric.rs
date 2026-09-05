//! ERIC — the US Department of Education's education research index.
//!
//! Education research is otherwise absent here entirely. ERIC is a Solr index
//! behind a thin JSON wrapper: no key, a true record offset, and a flag saying
//! whether ERIC hosts the full text itself.

use super::Paper;

/// Requested explicitly: ERIC returns only the fields you name.
const FIELDS: &str = "id,title,author,description,publicationdateyear,source,subject,url,peerreviewed,e_fulltextauth";

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    format!(
        "{}/?search={}&format=json&rows={}&start={}&fields={}",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        q.offset,
        FIELDS
    )
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    parse_search_response(&http_get(&build_search_url(base_url, q))?)
}

/// Fetch one record by its ERIC accession number (`ED...` or `EJ...`).
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/?search=id%3A{}&format=json&rows=1&fields={}",
        base_url,
        super::encode_query(identifier),
        FIELDS
    );
    Ok(parse_search_response(&http_get(&url)?)?.into_iter().next())
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
                last_err = "eric rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("eric server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("eric returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(err) = root["error"].as_str() {
        return Err(format!("eric API error: {}", err));
    }

    let docs = root["response"]["docs"]
        .as_array()
        .ok_or("missing 'response.docs' array in ERIC response")?;

    Ok(docs.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    let title = item["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }
    let id = item["id"].as_str().unwrap_or("").to_string();

    let authors = item["author"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let fields = item["subject"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // `e_fulltextauth` is 1 when ERIC hosts the PDF itself, at a path derived
    // from the accession number. Anything else has no file here, whatever its
    // copyright status elsewhere.
    let has_full_text = item["e_fulltextauth"].as_u64() == Some(1);
    let pdf_url = has_full_text.then(|| format!("https://files.eric.ed.gov/fulltext/{}.pdf", id));

    Some(Paper {
        id,
        title: title.to_string(),
        authors,
        abstract_text: item["description"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        year: item["publicationdateyear"]
            .as_u64()
            .map(|y| y as u16)
            .or_else(|| item["publicationdateyear"].as_str()?.parse().ok()),
        // ERIC indexes report literature and journal articles; it registers no
        // DOIs of its own and does not carry the publisher's.
        doi: None,
        url: item["url"].as_str().map(|s| s.to_string()),
        pdf_url,
        venue: item["source"].as_str().map(|s| s.to_string()),
        citations: None,
        fields,
        // Only the ERIC-hosted subset is knowably free to read.
        open_access: has_full_text.then_some(true),
        source: "eric".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/eric_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    // The fixture deliberately mixes ERIC-hosted records with ones ERIC only
    // indexes, since that split drives both pdf_url and open_access.
    #[test]
    fn parses_every_doc_in_the_fixture() {
        assert_eq!(parse_search_response(FIXTURE).unwrap().len(), 6);
    }

    #[test]
    fn sets_the_source_name() {
        assert!(
            parse_search_response(FIXTURE)
                .unwrap()
                .iter()
                .all(|p| p.source == "eric")
        );
    }

    #[test]
    fn reads_id_title_and_authors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers[0].id.starts_with("E"), "got: {}", papers[0].id);
        assert!(!papers[0].title.is_empty());
        assert!(papers.iter().any(|p| !p.authors.is_empty()));
    }

    #[test]
    fn reads_the_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    // The PDF path is derived from the accession number, but only for records
    // ERIC actually hosts; claiming one for the rest would 404 on download.
    #[test]
    fn only_records_flagged_as_eric_hosted_get_a_pdf_url() {
        let json = r#"{"response":{"docs":[
            {"id":"ED111","title":"Hosted","e_fulltextauth":1},
            {"id":"ED222","title":"Not hosted","e_fulltextauth":0}]}}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(
            papers[0].pdf_url.as_deref(),
            Some("https://files.eric.ed.gov/fulltext/ED111.pdf")
        );
        assert!(papers[1].pdf_url.is_none());
    }

    #[test]
    fn open_access_tracks_the_same_flag() {
        let json = r#"{"response":{"docs":[
            {"id":"ED111","title":"Hosted","e_fulltextauth":1},
            {"id":"ED222","title":"Not hosted","e_fulltextauth":0}]}}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].open_access, Some(true));
        assert!(papers[1].open_access.is_none());
    }

    #[test]
    fn the_fixture_covers_both_hosted_and_unhosted_records() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(
            papers.iter().any(|p| p.pdf_url.is_some()),
            "no hosted record"
        );
        assert!(
            papers.iter().any(|p| p.pdf_url.is_none()),
            "no unhosted record"
        );
    }

    #[test]
    fn reports_no_doi_and_no_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.doi.is_none()));
        assert!(papers.iter().all(|p| p.citations.is_none()));
    }

    // `source` and `url` are present on some records and absent on others.
    #[test]
    fn a_record_missing_source_and_url_still_parses() {
        let json = r#"{"response":{"docs":[{"id":"ED1","title":"T"}]}}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].venue.is_none());
        assert!(papers[0].url.is_none());
    }

    #[test]
    fn an_empty_doc_list_is_no_papers() {
        assert!(
            parse_search_response(r#"{"response":{"docs":[]}}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_record_with_no_title_is_skipped() {
        let json = r#"{"response":{"docs":[{"id":"ED1","title":""}]}}"#;
        assert!(parse_search_response(json).unwrap().is_empty());
    }

    #[test]
    fn a_body_with_no_docs_is_an_error() {
        assert!(parse_search_response(r#"{"meta":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    #[test]
    fn search_url_names_the_fields_the_parser_reads() {
        let url = build_search_url("https://api.ies.ed.gov/eric", &q("reading"));
        assert!(url.contains("e_fulltextauth"), "got: {}", url);
        assert!(url.contains("format=json"), "got: {}", url);
        assert!(url.contains("search=reading"), "got: {}", url);
    }

    // ERIC's `start` is a true record offset, so any value is valid.
    #[test]
    fn offset_passes_through_as_a_record_offset() {
        let mut query = q("reading");
        query.offset = 7;
        let url = build_search_url("https://api.ies.ed.gov/eric", &query);
        assert!(url.contains("start=7"), "got: {}", url);
    }
}
