//! INSPIRE-HEP — the high-energy physics literature database run by CERN,
//! DESY, Fermilab and SLAC.
//!
//! Its overlap with arXiv is near total for hep, so what it adds is not
//! coverage but *curation*: reliable citation counts, disambiguated authors,
//! and the link from a preprint to the journal version it became.
//!
//! Structured restrictions go through INSPIRE's own query language rather than
//! separate parameters — `a Ellis, John` for an author, `de 2015` for a year —
//! so the URL builder composes them into `q` with `and`.

use super::Paper;

const FIELDS: &str =
    "titles,authors,abstracts,dois,publication_info,citation_count,arxiv_eprints,earliest_date";

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let mut terms = vec![super::encode_query(&q.query)];

    // INSPIRE's `a` operator runs through its author disambiguation, so it
    // beats matching the name as free text.
    if let Some(ref author) = q.author {
        terms.push(format!("a+{}", super::encode_query(author)));
    }
    // `de` is the date of the earliest version.
    if let Some(year) = q.year {
        terms.push(format!("de+{}", year));
    }

    let page = page_for(q)?;
    let mut url = format!(
        "{}/api/literature?q={}&size={}&page={}&fields={}",
        base_url,
        terms.join("+and+"),
        q.limit,
        page,
        FIELDS
    );

    if let Some(sort) = q.sort {
        match sort {
            super::SortField::Relevance => {}
            super::SortField::Date => url.push_str("&sort=mostrecent"),
            super::SortField::Citations => url.push_str("&sort=mostcited"),
        }
    }

    Ok(url)
}

/// INSPIRE pages by number, so an offset only lands on a page boundary.
fn page_for(q: &super::SearchQuery) -> Result<u32, String> {
    if q.offset == 0 {
        return Ok(1);
    }
    if q.limit == 0 || !q.offset.is_multiple_of(q.limit) {
        return Err(format!(
            "inspire pages by page number, so --offset must be a multiple of -n ({}): got {}.",
            q.limit, q.offset
        ));
    }
    Ok(q.offset / q.limit + 1)
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;
    parse_search_response(&http_get(&url)?)
}

/// Fetch one record by INSPIRE control number or by DOI/arXiv id.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    // A bare control number addresses the record directly; anything else has to
    // go through a field-scoped search.
    let url = if identifier.chars().all(|c| c.is_ascii_digit()) {
        format!(
            "{}/api/literature/{}?fields={}",
            base_url, identifier, FIELDS
        )
    } else {
        let key = if identifier.starts_with("10.") {
            "doi"
        } else {
            "arxiv"
        };
        format!(
            "{}/api/literature?q={}+{}&size=1&fields={}",
            base_url,
            key,
            super::encode_query(identifier),
            FIELDS
        )
    };

    let body = match http_get(&url) {
        Ok(b) => b,
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let root: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {}", e))?;
    // The by-id form returns a bare record; the search form returns hits.
    if root["metadata"].is_object() {
        return Ok(parse_record(&root));
    }
    Ok(parse_search_response(&body)?.into_iter().next())
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
                last_err = "inspire rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => return Err("inspire returned 404".to_string()),
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("inspire server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("inspire returned HTTP {}", code));
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
        && root["status"].is_number()
    {
        return Err(format!("inspire API error: {}", msg));
    }

    let hits = root["hits"]["hits"]
        .as_array()
        .ok_or("missing 'hits.hits' array in INSPIRE response")?;

    Ok(hits.iter().filter_map(parse_record).collect())
}

fn parse_record(hit: &serde_json::Value) -> Option<Paper> {
    let m = &hit["metadata"];
    let title = m["titles"][0]["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }

    let id = hit["id"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| m["control_number"].as_u64().map(|n| n.to_string()))
        .unwrap_or_default();

    let authors = m["authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["full_name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // `publication_info.year` is the journal year; `earliest_date` covers
    // records that never reached a journal.
    let year = m["publication_info"][0]["year"]
        .as_u64()
        .map(|y| y as u16)
        .or_else(|| {
            m["earliest_date"]
                .as_str()
                .and_then(|d| d.get(..4))
                .and_then(|y| y.parse().ok())
        });

    // Nearly every hep record is on arXiv, and that is where its file actually
    // is. INSPIRE itself serves no PDF, so this points at the real one.
    let pdf_url = m["arxiv_eprints"][0]["value"]
        .as_str()
        .map(|e| format!("https://arxiv.org/pdf/{}", e));

    Some(Paper {
        id: id.clone(),
        title: title.to_string(),
        authors,
        abstract_text: m["abstracts"][0]["value"].as_str().map(|s| s.to_string()),
        year,
        doi: m["dois"][0]["value"].as_str().map(|s| s.to_string()),
        url: Some(format!("https://inspirehep.net/literature/{}", id)),
        pdf_url,
        venue: m["publication_info"][0]["journal_title"]
            .as_str()
            .map(|s| s.to_string()),
        citations: m["citation_count"].as_u64().map(|c| c as u32),
        fields: vec![],
        open_access: None,
        source: "inspire".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/inspire_search.json");

    fn q(query: &str) -> super::super::SearchQuery {
        super::super::SearchQuery::simple(query, 10)
    }

    #[test]
    fn parses_every_hit_in_the_fixture() {
        assert_eq!(parse_search_response(FIXTURE).unwrap().len(), 3);
    }

    #[test]
    fn sets_the_source_name() {
        assert!(
            parse_search_response(FIXTURE)
                .unwrap()
                .iter()
                .all(|p| p.source == "inspire")
        );
    }

    #[test]
    fn reads_title_authors_and_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers[0].title.is_empty());
        assert!(!papers[0].authors.is_empty());
        assert!(papers[0].doi.is_some());
    }

    #[test]
    fn reads_citation_counts() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(
            papers.iter().any(|p| p.citations.is_some()),
            "no citation counts parsed"
        );
    }

    #[test]
    fn takes_the_year_from_publication_info() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    #[test]
    fn points_the_pdf_at_the_arxiv_eprint() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let pdf = papers
            .iter()
            .find_map(|p| p.pdf_url.clone())
            .expect("no arXiv-backed pdf_url");
        assert!(pdf.starts_with("https://arxiv.org/pdf/"), "got: {}", pdf);
    }

    #[test]
    fn builds_the_record_url_from_the_id() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(
            papers[0].url,
            Some(format!(
                "https://inspirehep.net/literature/{}",
                papers[0].id
            ))
        );
    }

    #[test]
    fn says_nothing_about_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().all(|p| p.open_access.is_none()));
    }

    #[test]
    fn an_empty_hit_list_is_no_papers() {
        assert!(
            parse_search_response(r#"{"hits":{"hits":[],"total":0}}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_hit_with_no_title_is_skipped() {
        let json = r#"{"hits":{"hits":[{"id":"1","metadata":{"titles":[]}}]}}"#;
        assert!(parse_search_response(json).unwrap().is_empty());
    }

    #[test]
    fn absent_optional_fields_do_not_fail_parsing() {
        let json = r#"{"hits":{"hits":[{"id":"1","metadata":{"titles":[{"title":"T"}]}}]}}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].doi.is_none());
        assert!(papers[0].citations.is_none());
        assert!(papers[0].pdf_url.is_none());
    }

    #[test]
    fn a_body_with_no_hits_is_an_error() {
        assert!(parse_search_response(r#"{"meta":{}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    #[test]
    fn search_url_carries_query_size_and_fields() {
        let url = build_search_url("https://inspirehep.net", &q("higgs")).unwrap();
        assert!(url.contains("q=higgs"), "got: {}", url);
        assert!(url.contains("size=10"), "got: {}", url);
        assert!(url.contains("citation_count"), "got: {}", url);
    }

    #[test]
    fn author_becomes_the_a_operator() {
        let mut query = q("higgs");
        query.author = Some("Ellis, John".into());
        let url = build_search_url("https://inspirehep.net", &query).unwrap();
        assert!(url.contains("+and+a+Ellis%2C+John"), "got: {}", url);
    }

    #[test]
    fn year_becomes_the_de_operator() {
        let mut query = q("higgs");
        query.year = Some(2015);
        let url = build_search_url("https://inspirehep.net", &query).unwrap();
        assert!(url.contains("+and+de+2015"), "got: {}", url);
    }

    #[test]
    fn sort_by_date_is_mostrecent() {
        let mut query = q("higgs");
        query.sort = Some(super::super::SortField::Date);
        assert!(
            build_search_url("https://inspirehep.net", &query)
                .unwrap()
                .contains("sort=mostrecent")
        );
    }

    // INSPIRE is one of the few sources that really can order by citations.
    #[test]
    fn sort_by_citations_is_mostcited_not_an_error() {
        let mut query = q("higgs");
        query.sort = Some(super::super::SortField::Citations);
        assert!(
            build_search_url("https://inspirehep.net", &query)
                .unwrap()
                .contains("sort=mostcited")
        );
    }

    #[test]
    fn offset_becomes_a_page_number() {
        let mut query = q("higgs");
        query.offset = 30;
        assert!(
            build_search_url("https://inspirehep.net", &query)
                .unwrap()
                .contains("&page=4")
        );
    }

    #[test]
    fn an_offset_off_the_page_boundary_is_rejected() {
        let mut query = q("higgs");
        query.offset = 7;
        assert!(build_search_url("https://inspirehep.net", &query).is_err());
    }
}
