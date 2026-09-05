//! OSF Preprints — one API over ~32 preprint communities (PsyArXiv, SocArXiv,
//! EdArXiv, EcoEvoRxiv, engrXiv, MetaArXiv, Thesis Commons and the rest).
//!
//! This is the only source here that reaches social science, psychology,
//! education and law preprints, none of which arXiv, bioRxiv or medRxiv carry.
//!
//! Two shapes of the JSON:API response drive the parser. Authors live behind a
//! relationship, so every request asks for `embed=contributors` and reads them
//! out of `embeds`; and `attributes.doi` is null on essentially every record
//! while `links.preprint_doi` carries the real one as a `https://doi.org/...`
//! URL, so the DOI is derived from the link rather than the field named for it.

use super::Paper;

/// Free-text search is not available: the API rejects `filter[q]` outright, so
/// the query can only be matched against titles. Declared in `Capabilities`
/// as a plain search, but the caller deserves to see it in the URL builder too.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let page = page_for(q)?;

    let mut url = format!(
        "{}/v2/preprints/?filter%5Btitle%5D={}&page%5Bsize%5D={}&page={}&embed=contributors&embed=provider",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        page
    );

    // A provider is OSF's unit of subject scoping — `--field psyarxiv` is the
    // honest mapping, since OSF's own `subjects` taxonomy is not filterable.
    if let Some(ref field) = q.field {
        url.push_str(&format!(
            "&filter%5Bprovider%5D={}",
            super::encode_query(field)
        ));
    }

    // `--year N` is the closed interval inside that year; the API has no year
    // parameter of its own, only the two date bounds.
    if let Some(year) = q.year {
        url.push_str(&format!(
            "&filter%5Bdate_published%5D%5Bgte%5D={}-01-01&filter%5Bdate_published%5D%5Blte%5D={}-12-31",
            year, year
        ));
    }
    if let Some(ref after) = q.after {
        url.push_str(&format!(
            "&filter%5Bdate_published%5D%5Bgte%5D={}",
            super::validate_ymd(after)?
        ));
    }
    if let Some(ref before) = q.before {
        url.push_str(&format!(
            "&filter%5Bdate_published%5D%5Blte%5D={}",
            super::validate_ymd(before)?
        ));
    }

    if let Some(sort) = q.sort {
        match sort {
            super::SortField::Relevance => {}
            super::SortField::Date => {
                let field = match q.order {
                    super::SortOrder::Asc => "date_published",
                    super::SortOrder::Desc => "-date_published",
                };
                url.push_str(&format!("&sort={}", field));
            }
            super::SortField::Citations => {
                return Err("osf cannot sort by citations: OSF Preprints publishes no \
                     citation counts.\n\
                     Try semantic or openalex."
                    .to_string());
            }
        }
    }

    Ok(url)
}

/// OSF pages by number, not by record offset, so `--offset` only lands on a
/// page boundary. Saying so beats quietly returning the wrong window.
fn page_for(q: &super::SearchQuery) -> Result<u32, String> {
    if q.offset == 0 {
        return Ok(1);
    }
    if q.limit == 0 || !q.offset.is_multiple_of(q.limit) {
        return Err(format!(
            "osf pages by page number, so --offset must be a multiple of -n ({}): got {}.",
            q.limit, q.offset
        ));
    }
    Ok(q.offset / q.limit + 1)
}

/// Search OSF Preprints.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;
    parse_search_response(&http_get(&url)?)
}

/// Fetch one preprint by its OSF id (e.g. `2s7pw_v2`).
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/v2/preprints/{}/?embed=contributors&embed=provider",
        base_url,
        super::encode_query(identifier)
    );
    let body = match http_get(&url) {
        Ok(body) => body,
        // A miss is an empty answer, not a failure.
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    // The single-record form wraps one object where search returns an array.
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
                last_err = "osf rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => {
                return Err("osf returned 404".to_string());
            }
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("osf server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("osf returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

/// Parse an OSF JSON:API preprint list into `Paper`s.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    // JSON:API reports failures as a top-level `errors` array with a 200-shaped
    // body in some proxies; surface the detail rather than an empty result.
    if let Some(errors) = root["errors"].as_array()
        && let Some(detail) = errors.first().and_then(|e| e["detail"].as_str())
    {
        return Err(format!("osf API error: {}", detail));
    }

    let data = root["data"]
        .as_array()
        .ok_or("missing 'data' array in OSF response")?;

    Ok(data.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    let id = item["id"].as_str().unwrap_or("").to_string();
    let title = item["attributes"]["title"].as_str().unwrap_or("").trim();
    if id.is_empty() || title.is_empty() {
        return None;
    }

    let authors = item["embeds"]["contributors"]["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    c["embeds"]["users"]["data"]["attributes"]["full_name"]
                        .as_str()
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    // `attributes.doi` is null on nearly every record; the link is the real one.
    let doi = item["attributes"]["doi"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            item["links"]["preprint_doi"]
                .as_str()
                .and_then(|u| u.strip_prefix("https://doi.org/"))
                .map(|s| s.to_string())
        });

    let year = item["attributes"]["date_published"]
        .as_str()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse::<u16>().ok());

    let venue = item["embeds"]["provider"]["data"]["attributes"]["name"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            item["embeds"]["provider"]["data"]["id"]
                .as_str()
                .map(|s| s.to_string())
        });

    // `subjects` is an array of taxonomy *paths*; the last element of each path
    // is the specific term, and the leading ones are its ancestors.
    let mut fields: Vec<String> = item["attributes"]["subjects"]
        .as_array()
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.as_array()?.last()?["text"].as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    fields.dedup();

    let abstract_text = item["attributes"]["description"]
        .as_str()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Some(Paper {
        id: id.clone(),
        title: title.to_string(),
        authors,
        abstract_text,
        year,
        doi,
        url: item["links"]["html"].as_str().map(|s| s.to_string()),
        // OSF publishes no file URL in the record; this is its stable public
        // download path, and `download` builds the same path off its own base.
        pdf_url: Some(format!("https://osf.io/download/{}/", id)),
        venue,
        citations: None,
        fields,
        // Every OSF preprint is publicly readable.
        open_access: Some(true),
        source: "osf".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/osf_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    // ── parser ──────────────────────────────────

    #[test]
    fn parses_every_record_in_the_fixture() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers.len(), 3);
    }

    #[test]
    fn sets_the_source_name() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.source == "osf"));
    }

    #[test]
    fn reads_id_and_title() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].id, "2s7pw_v2");
        assert!(
            papers[0]
                .title
                .starts_with("Estimating Dimensional Structure"),
            "got: {}",
            papers[0].title
        );
    }

    // Authors are behind a JSON:API relationship; without `embed=contributors`
    // this list is silently empty, which is why the URL always asks for it.
    #[test]
    fn reads_authors_out_of_the_embedded_contributors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].authors.len(), 4);
        assert_eq!(papers[0].authors[0], "Luis Eduardo Garrido");
    }

    // The field named `doi` is null on every record in the fixture; the DOI has
    // to come off `links.preprint_doi` with the resolver prefix stripped.
    #[test]
    fn derives_the_doi_from_the_preprint_doi_link() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].doi.as_deref(), Some("10.31234/osf.io/2s7pw_v2"));
    }

    #[test]
    fn reads_year_from_the_publication_date() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].year, Some(2026));
    }

    #[test]
    fn uses_the_provider_display_name_as_the_venue() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].venue.as_deref(), Some("PsyArXiv"));
    }

    #[test]
    fn keeps_only_the_leaf_term_of_each_subject_path() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(
            papers[0].fields.contains(&"Psychometrics".to_string()),
            "got: {:?}",
            papers[0].fields
        );
        // "Social and Behavioral Sciences" is an ancestor, never a leaf here.
        assert!(
            !papers[0]
                .fields
                .contains(&"Social and Behavioral Sciences".to_string()),
            "ancestor leaked into fields: {:?}",
            papers[0].fields
        );
    }

    #[test]
    fn builds_the_public_download_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(
            papers[0].pdf_url.as_deref(),
            Some("https://osf.io/download/2s7pw_v2/")
        );
    }

    #[test]
    fn every_preprint_is_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.open_access == Some(true)));
    }

    #[test]
    fn carries_the_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers[0].abstract_text.as_ref().unwrap().len() > 100);
    }

    #[test]
    fn reports_no_citation_counts() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.citations.is_none()));
    }

    // ── tolerance ───────────────────────────────

    #[test]
    fn an_empty_data_array_is_no_papers_not_an_error() {
        let papers = parse_search_response(r#"{"data":[]}"#).unwrap();
        assert!(papers.is_empty());
    }

    #[test]
    fn a_record_with_no_title_is_skipped_rather_than_yielding_a_blank() {
        let json = r#"{"data":[{"id":"abc","attributes":{"title":""},"links":{}}]}"#;
        assert!(parse_search_response(json).unwrap().is_empty());
    }

    #[test]
    fn nulls_and_absent_keys_do_not_fail_parsing() {
        let json = r#"{"data":[{"id":"abc","attributes":{"title":"T","description":null,
                       "date_published":null,"doi":null,"subjects":null},"links":{}}]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].abstract_text.is_none());
        assert!(papers[0].year.is_none());
        assert!(papers[0].doi.is_none());
        assert!(papers[0].authors.is_empty());
    }

    #[test]
    fn a_json_api_error_body_becomes_an_err() {
        let json = r#"{"errors":[{"detail":"'q' is not a valid field for this endpoint."}]}"#;
        let err = parse_search_response(json).unwrap_err();
        assert!(err.contains("not a valid field"), "got: {}", err);
    }

    #[test]
    fn a_body_with_no_data_array_is_an_error() {
        assert!(parse_search_response(r#"{"meta":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("not json").is_err());
    }

    // ── URL building ────────────────────────────

    #[test]
    fn search_url_asks_for_the_embeds_the_parser_depends_on() {
        let url = build_search_url("https://api.osf.io", &q("attention")).unwrap();
        assert!(url.contains("embed=contributors"), "got: {}", url);
        assert!(url.contains("embed=provider"), "got: {}", url);
    }

    #[test]
    fn search_url_puts_the_query_in_the_title_filter() {
        let url = build_search_url("https://api.osf.io", &q("large language")).unwrap();
        assert!(
            url.contains("filter%5Btitle%5D=large+language"),
            "got: {}",
            url
        );
    }

    #[test]
    fn field_maps_onto_the_provider_filter() {
        let mut query = q("bias");
        query.field = Some("socarxiv".into());
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(
            url.contains("filter%5Bprovider%5D=socarxiv"),
            "got: {}",
            url
        );
    }

    #[test]
    fn year_becomes_a_closed_interval_of_date_bounds() {
        let mut query = q("bias");
        query.year = Some(2024);
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(url.contains("gte%5D=2024-01-01"), "got: {}", url);
        assert!(url.contains("lte%5D=2024-12-31"), "got: {}", url);
    }

    #[test]
    fn date_range_maps_onto_gte_and_lte() {
        let mut query = q("bias");
        query.after = Some("2023-01-01".into());
        query.before = Some("2023-06-30".into());
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(url.contains("gte%5D=2023-01-01"), "got: {}", url);
        assert!(url.contains("lte%5D=2023-06-30"), "got: {}", url);
    }

    #[test]
    fn a_malformed_date_is_rejected_before_the_request_goes_out() {
        let mut query = q("bias");
        query.after = Some("2023/01/01".into());
        assert!(build_search_url("https://api.osf.io", &query).is_err());
    }

    #[test]
    fn sort_by_date_descending_is_the_minus_prefixed_field() {
        let mut query = q("bias");
        query.sort = Some(super::super::SortField::Date);
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(url.contains("sort=-date_published"), "got: {}", url);
    }

    #[test]
    fn sort_by_date_ascending_drops_the_minus() {
        let mut query = q("bias");
        query.sort = Some(super::super::SortField::Date);
        query.order = super::super::SortOrder::Asc;
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(url.contains("sort=date_published"), "got: {}", url);
        assert!(!url.contains("sort=-date_published"), "got: {}", url);
    }

    #[test]
    fn sorting_by_citations_names_a_source_that_has_them() {
        let mut query = q("bias");
        query.sort = Some(super::super::SortField::Citations);
        let err = build_search_url("https://api.osf.io", &query).unwrap_err();
        assert!(err.contains("semantic"), "got: {}", err);
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut query = q("bias");
        query.offset = 20;
        let url = build_search_url("https://api.osf.io", &query).unwrap();
        assert!(url.contains("&page=3"), "got: {}", url);
    }

    #[test]
    fn an_offset_off_the_page_boundary_is_rejected_with_the_reason() {
        let mut query = q("bias");
        query.offset = 15;
        let err = build_search_url("https://api.osf.io", &query).unwrap_err();
        assert!(err.contains("multiple of -n"), "got: {}", err);
    }
}
