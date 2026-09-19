//! OAPEN Library — peer-reviewed open access books and chapters, mostly
//! humanities and social sciences.
//!
//! A DSpace REST API. Its documentation page sits behind Cloudflare, so
//! everything here is as measured on 2026-09-19: the search takes Solr-style
//! `dc.*` clauses, the index also holds funder records (`dc.type:grantor`, no
//! title) that are not publications, and `limit=500` took 19 s against a 30 s
//! request budget.

use super::Paper;

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

/// Where the files are, whatever base URL the API was reached through.
const LIBRARY: &str = "https://library.oapen.org";

fn build_query(q: &super::SearchQuery) -> String {
    let mut query = format!("({})", q.query.trim());
    if let Some(year) = q.year {
        query.push_str(&format!(" AND dc.date.issued:{}", year));
    }
    if let Some(ref author) = q.author {
        query.push_str(&format!(
            " AND dc.contributor.author:\"{}\"",
            author.replace('"', "")
        ));
    }
    query.push_str(" AND NOT dc.type:grantor");
    query
}

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    let mut url = format!(
        "{}/rest/search?query={}&limit={}",
        base_url.trim_end_matches('/'),
        super::encode_query(&build_query(q)),
        q.limit
    );
    if q.offset > 0 {
        url.push_str(&format!("&offset={}", q.offset));
    }
    url.push_str("&expand=metadata,bitstreams");
    url
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    parse_search_response(&http_get(&build_search_url(base_url, q))?)
}

/// Fetch an item's metadata and bitstreams by handle.
///
/// Returns `Ok(None)` if the handle does not exist (404), `Ok(Some(body))` if
/// found, or `Err` if the request failed (including Cloudflare challenges).
pub fn fetch_item(base_url: &str, handle: &str) -> Result<Option<String>, String> {
    let url = format!(
        "{}/rest/handle/{}?expand=metadata,bitstreams",
        base_url.trim_end_matches('/'),
        handle.trim()
    );
    match http_get(&url) {
        Ok(body) => Ok(Some(body)),
        Err(e) if e.contains("oapen returned 404") => Ok(None),
        Err(e) => Err(e),
    }
}

/// Fetch one book or chapter by its handle, e.g. `20.500.12657/98246`.
pub fn get_by_id(base_url: &str, handle: &str) -> Result<Option<Paper>, String> {
    match fetch_item(base_url, handle)? {
        None => Ok(None),
        Some(body) => parse_item_response(&body),
    }
}

fn http_get(url: &str) -> Result<String, String> {
    let resp = crate::http::api()
        .get(url)
        .config()
        .http_status_as_error(false)
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;
    let status = resp.status().as_u16();
    let body = resp
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;
    // Check 404 first: a 404 means not found regardless of body.
    if status == 404 {
        return Err("oapen returned 404".to_string());
    }
    // Then sniff for HTML body (Cloudflare challenge or outage page).
    if body.trim_start().starts_with('<') {
        return Err(format!(
            "OAPEN answered HTTP {} with an HTML page instead of JSON (a Cloudflare challenge \
             or an outage page).",
            status
        ));
    }
    // Finally check if status is not 2xx.
    if !(200..300).contains(&status) {
        return Err(format!("oapen returned HTTP {}", status));
    }
    Ok(body)
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    let items = root
        .as_array()
        .ok_or("expected a JSON array of items from OAPEN")?;
    Ok(items.iter().filter_map(parse_item).collect())
}

pub fn parse_item_response(json: &str) -> Result<Option<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(parse_item(&root))
}

/// The retrieve path of an item's PDF, e.g. `/rest/bitstreams/<uuid>/retrieve`.
pub fn pdf_link(json: &str) -> Result<Option<String>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(pdf_path(&root).map(str::to_string))
}

fn pdf_path(item: &serde_json::Value) -> Option<&str> {
    item["bitstreams"]
        .as_array()?
        .iter()
        .find(|b| b["mimeType"] == "application/pdf")?["retrieveLink"]
        .as_str()
}

fn parse_item(item: &serde_json::Value) -> Option<Paper> {
    let values = |key: &str| -> Vec<String> {
        item["metadata"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|m| m["key"] == key)
                    .filter_map(|m| m["value"].as_str())
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    let first = |key: &str| values(key).into_iter().next();

    // Funder records share the index; only books and chapters are publications.
    let kind = values("dc.type").join(" ").to_lowercase();
    if !(kind.contains("book") || kind.contains("chapter")) {
        return None;
    }
    let title = first("dc.title")?;
    let handle = item["handle"].as_str()?.to_string();

    let mut authors = values("dc.contributor.author");
    if authors.is_empty() {
        authors = values("dc.contributor.editor")
            .into_iter()
            .map(|name| format!("{} (ed.)", name))
            .collect();
    }

    Some(Paper {
        id: handle.clone(),
        title,
        authors,
        abstract_text: first("dc.description.abstract"),
        year: first("dc.date.issued").and_then(|d| d.get(..4)?.parse().ok()),
        doi: first("oapen.identifier.doi"),
        url: Some(format!("{}/handle/{}", LIBRARY, handle)),
        pdf_url: pdf_path(item).map(|p| format!("{}{}", LIBRARY, p)),
        venue: first("publisher.name"),
        citations: None,
        // "thema EDItEUR::U Computing…::UYQ Artificial intelligence" -> the leaf.
        fields: values("dc.subject.classification")
            .iter()
            .filter_map(|c| c.rsplit("::").next())
            .map(str::to_string)
            .collect(),
        open_access: Some(true),
        source: "oapen".to_string(),
        community: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SearchQuery;

    const SEARCH: &str = include_str!("../../tests/fixtures/oapen_search.json");
    const ITEM: &str = include_str!("../../tests/fixtures/oapen_item.json");

    fn papers() -> Vec<Paper> {
        parse_search_response(SEARCH).unwrap()
    }

    fn by_id(id: &str) -> Paper {
        papers().into_iter().find(|p| p.id == id).unwrap()
    }

    #[test]
    fn funder_records_are_not_publications() {
        assert_eq!(papers().len(), 4);
        assert!(papers().iter().all(|p| p.id != "20.500.12657/66016"));
    }

    #[test]
    fn a_book_maps_its_metadata() {
        let p = by_id("20.500.12657/98246");
        assert_eq!(p.title, "Practical Machine Learning");
        assert_eq!(p.authors.len(), 6);
        assert_eq!(p.year, Some(2025));
        assert_eq!(p.doi.as_deref(), Some("10.1201/9781003486817"));
        assert_eq!(p.venue.as_deref(), Some("Taylor & Francis"));
        assert_eq!(
            p.url.as_deref(),
            Some("https://library.oapen.org/handle/20.500.12657/98246")
        );
        assert_eq!(
            p.pdf_url.as_deref(),
            Some(
                "https://library.oapen.org/rest/bitstreams/7ed5481a-3418-4316-b28e-d092d0b8df20/retrieve"
            )
        );
        assert!(
            p.fields
                .contains(&"UYQ Artificial intelligence".to_string()),
            "{:?}",
            p.fields
        );
        assert_eq!(p.open_access, Some(true));
        assert!(p.abstract_text.is_some());
    }

    #[test]
    fn an_edited_volume_lists_its_editors() {
        assert_eq!(
            by_id("20.500.12657/97300").authors,
            vec!["Hajian, Aram (ed.)", "Baloian, Nelson (ed.)"]
        );
    }

    #[test]
    fn chapters_are_kept() {
        assert!(by_id("20.500.12657/49384").title.starts_with("Chapter"));
    }

    #[test]
    fn a_record_without_bitstreams_has_no_pdf_url() {
        assert!(by_id("20.500.12657/49384").pdf_url.is_none());
    }

    #[test]
    fn a_single_item_parses_the_same_way() {
        let p = parse_item_response(ITEM).unwrap().unwrap();
        assert_eq!(p.id, "20.500.12657/98246");
        assert_eq!(
            pdf_link(ITEM).unwrap().as_deref(),
            Some("/rest/bitstreams/7ed5481a-3418-4316-b28e-d092d0b8df20/retrieve")
        );
    }

    #[test]
    fn the_query_always_drops_funder_records() {
        assert_eq!(
            build_query(&SearchQuery::simple("machine learning", 10)),
            "(machine learning) AND NOT dc.type:grantor"
        );
    }

    #[test]
    fn year_and_author_become_field_clauses() {
        let mut q = SearchQuery::simple("climate", 10);
        q.year = Some(2023);
        q.author = Some("Latour".into());
        assert_eq!(
            build_query(&q),
            "(climate) AND dc.date.issued:2023 AND dc.contributor.author:\"Latour\" AND NOT dc.type:grantor"
        );
    }

    #[test]
    fn search_url_carries_limit_offset_and_expand() {
        let mut q = SearchQuery::simple("history", 5);
        q.offset = 5;
        let url = build_search_url("https://library.oapen.org", &q);
        assert!(
            url.starts_with("https://library.oapen.org/rest/search?query="),
            "{}",
            url
        );
        assert!(
            url.contains("&limit=5&offset=5&expand=metadata,bitstreams"),
            "{}",
            url
        );
    }

    #[test]
    fn a_non_array_body_is_an_error() {
        assert!(parse_search_response(r#"{"error":"x"}"#).is_err());
    }
}
