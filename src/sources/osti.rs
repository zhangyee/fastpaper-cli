//! OSTI.GOV — the US Department of Energy's research output: technical
//! reports, theses, conference papers and datasets.
//!
//! Technical-report grey literature is otherwise unreachable here. OSTI serves
//! the full text itself at a stable `servlets/purl/<id>` path whenever the
//! record carries a `fulltext` link.

use super::Paper;

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let page = page_for(q)?;
    let mut url = format!(
        "{}/api/v1/records?q={}&rows={}&page={}",
        base_url,
        super::encode_query(&q.query),
        q.limit,
        page
    );

    // OSTI takes US-formatted dates on these two parameters.
    if let Some(year) = q.year {
        url.push_str(&format!(
            "&publication_date_start=01%2F01%2F{}&publication_date_end=12%2F31%2F{}",
            year, year
        ));
    }
    if let Some(ref after) = q.after {
        url.push_str(&format!(
            "&publication_date_start={}",
            us_date(super::validate_ymd(after)?)
        ));
    }
    if let Some(ref before) = q.before {
        url.push_str(&format!(
            "&publication_date_end={}",
            us_date(super::validate_ymd(before)?)
        ));
    }

    if let Some(sort) = q.sort {
        match sort {
            super::SortField::Relevance => {}
            super::SortField::Date => {
                let dir = match q.order {
                    super::SortOrder::Asc => "asc",
                    super::SortOrder::Desc => "desc",
                };
                url.push_str(&format!("&sort=publication_date%20{}", dir));
            }
            super::SortField::Citations => {
                return Err(
                    "osti cannot sort by citations: it indexes no citation counts.\n\
                     Try semantic or openalex."
                        .to_string(),
                );
            }
        }
    }

    Ok(url)
}

/// `YYYY-MM-DD` -> `MM%2FDD%2FYYYY`, which is the only form OSTI accepts.
fn us_date(ymd: &str) -> String {
    let (y, rest) = ymd.split_at(4);
    let m = &rest[1..3];
    let d = &rest[4..6];
    format!("{}%2F{}%2F{}", m, d, y)
}

fn page_for(q: &super::SearchQuery) -> Result<u32, String> {
    if q.offset == 0 {
        return Ok(1);
    }
    if q.limit == 0 || !q.offset.is_multiple_of(q.limit) {
        return Err(format!(
            "osti pages by page number, so --offset must be a multiple of -n ({}): got {}.",
            q.limit, q.offset
        ));
    }
    Ok(q.offset / q.limit + 1)
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    parse_search_response(&http_get(&build_search_url(base_url, q)?)?)
}

/// Fetch one record by OSTI id.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/api/v1/records/{}",
        base_url,
        super::encode_query(identifier)
    );
    let body = match http_get(&url) {
        Ok(b) => b,
        Err(e) if e.contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
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
                last_err = "osti rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(404)) => return Err("osti returned 404".to_string()),
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("osti server error: {}", code));
            }
            Err(ureq::Error::StatusCode(code)) => {
                return Err(format!("osti returned HTTP {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

/// OSTI answers with a bare array of records, or a single object for `get`.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(err) = root["errors"].as_str().or_else(|| root["error"].as_str()) {
        return Err(format!("osti API error: {}", err));
    }

    let records = match root.as_array() {
        Some(arr) => arr.clone(),
        None if root.is_object() => vec![root.clone()],
        None => return Err("unexpected OSTI response shape".to_string()),
    };

    Ok(records.iter().filter_map(parse_record).collect())
}

fn parse_record(item: &serde_json::Value) -> Option<Paper> {
    let title = item["title"].as_str().unwrap_or("").trim();
    if title.is_empty() {
        return None;
    }
    let id = item["osti_id"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| item["osti_id"].as_u64().map(|n| n.to_string()))
        .unwrap_or_default();

    // OSTI appends the affiliation to the name inside square brackets:
    // "Yan, Feng [Materials Science ... Arizona State University]".
    let authors = item["authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.as_str())
                .map(|a| a.split(" [").next().unwrap_or(a).trim().to_string())
                .filter(|a| !a.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // `links` is a typed list; the fulltext entry is the only downloadable one.
    let pdf_url = item["links"].as_array().and_then(|links| {
        links
            .iter()
            .find(|l| l["rel"].as_str() == Some("fulltext"))
            .and_then(|l| l["href"].as_str())
            .map(|s| s.to_string())
    });

    Some(Paper {
        id,
        title: title.to_string(),
        authors,
        abstract_text: item["description"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        year: item["publication_date"]
            .as_str()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok()),
        doi: item["doi"].as_str().map(|s| s.to_string()),
        url: item["links"].as_array().and_then(|links| {
            links
                .iter()
                .find(|l| l["rel"].as_str() == Some("citation"))
                .and_then(|l| l["href"].as_str())
                .map(|s| s.to_string())
        }),
        // A fulltext link is exactly what makes the record readable.
        open_access: Some(pdf_url.is_some()),
        pdf_url,
        venue: item["journal_name"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| item["product_type"].as_str().map(|s| s.to_string())),
        citations: None,
        fields: vec![],
        source: "osti".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/osti_search.json");

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
                .all(|p| p.source == "osti")
        );
    }

    #[test]
    fn reads_id_title_and_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers[0].id.is_empty());
        assert!(!papers[0].title.is_empty());
        assert!(papers.iter().any(|p| p.doi.is_some()));
    }

    // The affiliation rides along inside the name field and has to come off.
    #[test]
    fn strips_the_bracketed_affiliation_from_author_names() {
        let json = r#"{"osti_id":"1","title":"T","authors":
            ["Yan, Feng [Arizona State University, Tempe, AZ]","Plain, Name"]}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].authors, vec!["Yan, Feng", "Plain, Name"]);
    }

    #[test]
    fn takes_the_pdf_from_the_fulltext_link() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let pdf = papers
            .iter()
            .find_map(|p| p.pdf_url.clone())
            .expect("no fulltext link parsed");
        assert!(pdf.contains("servlets/purl"), "got: {}", pdf);
    }

    // A record with no fulltext link is not readable here, and must not claim
    // to be open access.
    #[test]
    fn open_access_follows_the_presence_of_a_fulltext_link() {
        let json = r#"[{"osti_id":"1","title":"Has text","links":[{"rel":"fulltext","href":"https://osti.gov/x"}]},
                       {"osti_id":"2","title":"No text","links":[{"rel":"citation","href":"https://osti.gov/y"}]}]"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers[0].open_access, Some(true));
        assert_eq!(papers[1].open_access, Some(false));
        assert!(papers[1].pdf_url.is_none());
    }

    #[test]
    fn reads_the_year_from_the_publication_date() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.iter().any(|p| p.year.is_some()));
    }

    // `get` addresses one record and gets an object back, not an array.
    #[test]
    fn a_single_object_response_parses_as_one_paper() {
        let json = r#"{"osti_id":"9","title":"Single"}"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].title, "Single");
    }

    #[test]
    fn an_empty_array_is_no_papers() {
        assert!(parse_search_response("[]").unwrap().is_empty());
    }

    #[test]
    fn a_record_with_no_title_is_skipped() {
        assert!(
            parse_search_response(r#"[{"osti_id":"1","title":""}]"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn nulls_and_absent_keys_do_not_fail_parsing() {
        let json = r#"[{"osti_id":"1","title":"T","authors":null,"links":null,"doi":null}]"#;
        let papers = parse_search_response(json).unwrap();
        assert_eq!(papers.len(), 1);
        assert!(papers[0].authors.is_empty());
        assert!(papers[0].doi.is_none());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_search_response("nope").is_err());
    }

    #[test]
    fn search_url_carries_query_rows_and_page() {
        let url = build_search_url("https://www.osti.gov", &q("solar")).unwrap();
        assert!(url.contains("q=solar"), "got: {}", url);
        assert!(url.contains("rows=10"), "got: {}", url);
        assert!(url.contains("page=1"), "got: {}", url);
    }

    #[test]
    fn year_becomes_a_us_formatted_date_range() {
        let mut query = q("solar");
        query.year = Some(2024);
        let url = build_search_url("https://www.osti.gov", &query).unwrap();
        assert!(
            url.contains("publication_date_start=01%2F01%2F2024"),
            "got: {}",
            url
        );
        assert!(
            url.contains("publication_date_end=12%2F31%2F2024"),
            "got: {}",
            url
        );
    }

    #[test]
    fn iso_dates_are_reordered_into_the_us_form_osti_requires() {
        let mut query = q("solar");
        query.after = Some("2023-04-05".into());
        let url = build_search_url("https://www.osti.gov", &query).unwrap();
        assert!(
            url.contains("publication_date_start=04%2F05%2F2023"),
            "got: {}",
            url
        );
    }

    #[test]
    fn a_malformed_date_is_rejected_before_the_request_goes_out() {
        let mut query = q("solar");
        query.after = Some("05/04/2023".into());
        assert!(build_search_url("https://www.osti.gov", &query).is_err());
    }

    #[test]
    fn sort_by_date_uses_the_publication_date_field() {
        let mut query = q("solar");
        query.sort = Some(super::super::SortField::Date);
        assert!(
            build_search_url("https://www.osti.gov", &query)
                .unwrap()
                .contains("sort=publication_date%20desc")
        );
    }

    #[test]
    fn sorting_by_citations_names_a_source_that_has_them() {
        let mut query = q("solar");
        query.sort = Some(super::super::SortField::Citations);
        let err = build_search_url("https://www.osti.gov", &query).unwrap_err();
        assert!(err.contains("semantic"), "got: {}", err);
    }

    #[test]
    fn an_offset_off_the_page_boundary_is_rejected() {
        let mut query = q("solar");
        query.offset = 3;
        assert!(build_search_url("https://www.osti.gov", &query).is_err());
    }
}
