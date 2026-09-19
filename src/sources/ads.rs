//! NASA ADS (being renamed SciX) — astronomy and physics, with citation edges.
//!
//! Besides unpaywall, the one source that will not answer without
//! configuration: every request needs a token from `ADS_API_TOKEN` (free, from
//! the SciX account settings). The query passes through as ADS/Solr syntax.
//! Measured 2026-09-19 with a real token: `title`, `author`, `doi` and
//! `identifier` are arrays, keys with no value are absent, and the `doi`
//! array mixes in the arXiv DOI (`10.48550/…`).

use super::{Paper, SortField, SortOrder};

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

pub(crate) const FIELDS: &str =
    "bibcode,title,author,abstract,year,doi,pub,citation_count,identifier,property,esources,keyword";

pub(crate) fn token() -> Result<String, String> {
    std::env::var("ADS_API_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            "ads needs an API token in ADS_API_TOKEN.\n\
             Get one free: sign in at https://scixplorer.org, then \
             https://scixplorer.org/user/settings/token"
                .to_string()
        })
}

pub fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    let mut url = format!(
        "{}/search/query?q={}&fl={}&rows={}&start={}",
        base_url.trim_end_matches('/'),
        super::encode_query(&q.query),
        FIELDS,
        q.limit,
        q.offset
    );
    let mut fq = |clause: String| url.push_str(&format!("&fq={}", super::encode_query(&clause)));
    if let Some(ref author) = q.author {
        fq(format!("author:\"{}\"", author.replace('"', "")));
    }
    if let Some(year) = q.year {
        fq(format!("year:{}", year));
    }
    // `--field` names an ADS database: astronomy, physics or general.
    if let Some(ref database) = q.field {
        fq(format!("database:{}", database));
    }
    if q.open_access {
        fq("property:openaccess".to_string());
    }
    if let Some(sort) = q.sort {
        let field = match sort {
            SortField::Relevance => "score",
            SortField::Date => "date",
            SortField::Citations => "citation_count",
        };
        let order = match q.order {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        };
        url.push_str(&format!("&sort={}", super::encode_query(&format!("{} {}", field, order))));
    }
    url
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let token = token()?;
    parse_search_response(&http_get(&build_search_url(base_url, q), &token)?)
}

/// Fetch one record by bibcode, DOI or arXiv id; ADS files all three under
/// `identifier`.
pub fn get_by_id(base_url: &str, id: &str) -> Result<Option<Paper>, String> {
    let token = token()?;
    let url = format!(
        "{}/search/query?q={}&fl={}&rows=1",
        base_url.trim_end_matches('/'),
        super::encode_query(&format!("identifier:\"{}\"", identifier_for(id))),
        FIELDS
    );
    Ok(parse_search_response(&http_get(&url, &token)?)?.into_iter().next())
}

/// ADS files arXiv ids as `arXiv:<id>`; bibcodes and DOIs go in as given.
pub(crate) fn identifier_for(id: &str) -> String {
    use crate::identifier::{IdType, detect_id_type};
    let id = id.trim();
    if id.starts_with("arXiv:") {
        return id.to_string();
    }
    match detect_id_type(id) {
        IdType::Arxiv | IdType::ArxivOld => format!("arXiv:{}", id),
        _ => id.to_string(),
    }
}

pub(crate) fn http_get(url: &str, token: &str) -> Result<String, String> {
    let resp = crate::http::api()
        .get(url)
        .config()
        .http_status_as_error(false)
        .build()
        .header("User-Agent", USER_AGENT)
        .header("Authorization", &format!("Bearer {}", token))
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;
    let status = resp.status().as_u16();
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let remaining = header("x-ratelimit-remaining");
    let reset = header("x-ratelimit-reset");
    let body = resp
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    match status {
        200..=299 => Ok(body),
        401 => Err(format!(
            "ads rejected ADS_API_TOKEN (401): {}\nGenerate a new token at https://scixplorer.org/user/settings/token",
            api_message(&body)
        )),
        429 => Err(quota_message(reset.as_deref())),
        _ if remaining.as_deref() == Some("0") => Err(quota_message(reset.as_deref())),
        _ => Err(format!("ads returned HTTP {}: {}", status, api_message(&body))
            .trim_end_matches([':', ' '])
            .to_string()),
    }
}

/// The daily quota is per token and resets at 00:00 UTC; retrying sooner
/// cannot succeed, so this is not retried.
fn quota_message(reset: Option<&str>) -> String {
    format!(
        "ads daily quota is used up (5000 searches a day per token); it resets at 00:00 UTC{}. \
         Retrying before then will not help.",
        reset
            .and_then(|r| r.parse::<u64>().ok())
            .map(|epoch| format!(" (Unix time {})", epoch))
            .unwrap_or_default()
    )
}

fn api_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v["message"]
                .as_str()
                .or_else(|| v["error"]["msg"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    if let Some(msg) = root["error"]["msg"].as_str() {
        return Err(format!("ads query error: {}", msg));
    }
    let docs = root["response"]["docs"]
        .as_array()
        .ok_or("missing 'response.docs' in ADS response")?;
    Ok(docs.iter().filter_map(parse_doc).collect())
}

fn parse_doc(d: &serde_json::Value) -> Option<Paper> {
    let bibcode = d["bibcode"].as_str()?.to_string();
    let title = d["title"][0]
        .as_str()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())?;
    let strings = |key: &str| -> Vec<String> {
        d[key]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let doi = strings("doi")
        .into_iter()
        .find(|x| !x.to_lowercase().starts_with("10.48550/"));
    let arxiv = strings("identifier")
        .into_iter()
        .find_map(|i| i.strip_prefix("arXiv:").map(str::to_string));
    let open_access = d["property"]
        .is_array()
        .then(|| strings("property").iter().any(|p| p == "OPENACCESS"));

    Some(Paper {
        id: bibcode.clone(),
        title,
        authors: strings("author"),
        abstract_text: d["abstract"].as_str().map(str::to_string),
        year: d["year"].as_str().and_then(|y| y.parse().ok()),
        doi,
        url: Some(format!("https://ui.adsabs.harvard.edu/abs/{}", bibcode)),
        pdf_url: arxiv.map(|a| format!("https://arxiv.org/pdf/{}", a)),
        venue: d["pub"].as_str().map(str::to_string),
        citations: d["citation_count"].as_u64().map(|c| c as u32),
        fields: strings("keyword"),
        open_access,
        source: "ads".to_string(),
        community: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField, SortOrder};

    const SEARCH: &str = include_str!("../../tests/fixtures/ads_search.json");

    fn papers() -> Vec<Paper> {
        parse_search_response(SEARCH).unwrap()
    }

    #[test]
    fn parses_every_doc() {
        assert_eq!(papers().len(), 3);
        assert!(papers().iter().all(|p| p.source == "ads"));
    }

    #[test]
    fn maps_a_journal_article() {
        let p = &papers()[0];
        assert_eq!(p.id, "2022ApJ...930L..12E");
        assert!(p.title.starts_with("First Sagittarius A* Event Horizon Telescope Results"));
        assert_eq!(p.year, Some(2022));
        assert_eq!(p.venue.as_deref(), Some("The Astrophysical Journal Letters"));
        assert_eq!(p.citations, Some(2224));
        assert_eq!(p.open_access, Some(true));
        assert_eq!(p.url.as_deref(), Some("https://ui.adsabs.harvard.edu/abs/2022ApJ...930L..12E"));
        assert!(!p.fields.is_empty());
    }

    // The doi array mixes in the arXiv DOI; the publisher's is the one to keep.
    #[test]
    fn the_arxiv_doi_is_not_the_doi() {
        assert_eq!(papers()[0].doi.as_deref(), Some("10.3847/2041-8213/ac6674"));
        assert_eq!(papers()[1].doi.as_deref(), Some("10.1002/9783527617661"));
        assert_eq!(papers()[2].doi, None);
    }

    #[test]
    fn the_pdf_url_is_the_arxiv_copy_when_there_is_one() {
        assert_eq!(papers()[0].pdf_url.as_deref(), Some("https://arxiv.org/pdf/2311.08680"));
        assert_eq!(papers()[1].pdf_url, None);
    }

    #[test]
    fn open_access_follows_the_property_list() {
        assert_eq!(papers()[1].open_access, Some(false));
        assert_eq!(papers()[2].open_access, Some(true));
    }

    #[test]
    fn a_query_error_is_an_error() {
        assert!(parse_search_response(r#"{"error":{"msg":"syntax error"}}"#)
            .unwrap_err()
            .contains("syntax error"));
    }

    #[test]
    fn search_url_puts_each_filter_in_its_own_fq() {
        let mut q = SearchQuery::simple("dark energy", 5);
        q.author = Some("Perlmutter, S".into());
        q.year = Some(2020);
        q.field = Some("astronomy".into());
        q.open_access = true;
        q.offset = 10;
        let url = build_search_url("https://api.adsabs.harvard.edu/v1", &q);
        assert!(url.starts_with("https://api.adsabs.harvard.edu/v1/search/query?q=dark+energy&fl="), "{}", url);
        assert!(url.contains("&rows=5&start=10"), "{}", url);
        assert!(url.contains("&fq=author%3A%22Perlmutter%2C+S%22"), "{}", url);
        assert!(url.contains("&fq=year%3A2020"), "{}", url);
        assert!(url.contains("&fq=database%3Aastronomy"), "{}", url);
        assert!(url.contains("&fq=property%3Aopenaccess"), "{}", url);
    }

    #[test]
    fn sort_maps_onto_ads_fields() {
        let mut q = SearchQuery::simple("x", 5);
        q.sort = Some(SortField::Citations);
        q.order = SortOrder::Desc;
        assert!(build_search_url("b", &q).contains("&sort=citation_count+desc"));
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(build_search_url("b", &q).contains("&sort=date+asc"));
        q.sort = Some(SortField::Relevance);
        assert!(build_search_url("b", &q).contains("&sort=score+asc"));
    }

    #[test]
    fn arxiv_ids_get_the_prefix_ads_files_them_under() {
        assert_eq!(identifier_for("1805.00001"), "arXiv:1805.00001");
        assert_eq!(identifier_for("arXiv:1805.00001"), "arXiv:1805.00001");
        assert_eq!(identifier_for("2022ApJ...930L..12E"), "2022ApJ...930L..12E");
        assert_eq!(identifier_for("10.3847/2041-8213/ac6674"), "10.3847/2041-8213/ac6674");
    }

    #[test]
    #[serial_test::serial]
    fn no_token_is_refused_before_any_request() {
        unsafe { std::env::remove_var("ADS_API_TOKEN") };
        let err = search("http://127.0.0.1:9", &SearchQuery::simple("x", 1)).unwrap_err();
        assert!(err.contains("ADS_API_TOKEN") && err.contains("scixplorer.org"), "{}", err);
    }
}
