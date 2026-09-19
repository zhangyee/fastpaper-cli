//! OpenReview — conference submissions (ICLR, NeurIPS, …) with their outcome.
//!
//! Only `notes/search` is open. Reading one note (`/notes?id=`) and every PDF
//! address answer 403 `ChallengeRequiredError` (measured 2026-09-19), and on
//! 2026-09-05 search did too, so this source is search-only and may close
//! again. What it has that dblp lacks is the decision: `venue` reads
//! "ICLR 2025 Poster", "Submitted to ICLR 2024" (rejected) or
//! "… Withdrawn Submission".
//!
//! Without `source=forum` the search returns reviews and comments first; the
//! API also ignores `sort` (measured: identical order with and without it).

use super::Paper;

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    let mut url = format!(
        "{}/notes/search?term={}&source=forum&limit={}",
        base_url.trim_end_matches('/'),
        super::encode_query(&q.query),
        q.limit
    );
    if q.offset > 0 {
        url.push_str(&format!("&offset={}", q.offset));
    }
    // `--field` names an OpenReview group: one edition or a whole venue.
    if let Some(ref group) = q.field {
        url.push_str(&format!("&group={}", super::encode_query(group)));
    }
    url
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    parse_search_response(&http_get(&build_search_url(base_url, q))?)
}

fn http_get(url: &str) -> Result<String, String> {
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(u64::from(attempt)));
        }
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
        match status {
            200..=299 => return Ok(body),
            429 => continue,
            403 if body.contains("ChallengeRequiredError") => {
                return Err(
                    "OpenReview answered search with a bot challenge (403 ChallengeRequiredError), \
                     as it did before 2026-09-16. Search dblp or semantic for the same papers."
                        .to_string(),
                );
            }
            _ => return Err(format!("openreview returned HTTP {}", status)),
        }
    }
    Err("openreview rate limited (429) after 3 attempts".to_string())
}

pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    if let Some(name) = root["name"].as_str()
        && root["status"].is_number()
    {
        return Err(format!(
            "openreview API error: {}: {}",
            name,
            root["message"].as_str().unwrap_or("")
        ));
    }
    let notes = root["notes"]
        .as_array()
        .ok_or("missing 'notes' array in OpenReview response")?;
    Ok(notes.iter().filter_map(parse_note).collect())
}

fn parse_note(note: &serde_json::Value) -> Option<Paper> {
    let c = &note["content"];
    let text = |key: &str| {
        c[key]["value"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let title = text("title")?;
    let id = note["id"].as_str()?.to_string();

    let authors = c["authors"]["value"]
        .as_array()
        .map(|arr| arr.iter().filter_map(author_name).collect())
        .unwrap_or_default();
    let fields = c["keywords"]["value"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|k| k.as_str())
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let millis = note["pdate"].as_i64().or_else(|| note["cdate"].as_i64());
    let pdf_url = text("pdf").map(|p| {
        if p.starts_with('/') {
            format!("https://openreview.net{}", p)
        } else {
            p
        }
    });

    Some(Paper {
        id: id.clone(),
        title,
        authors,
        abstract_text: text("abstract"),
        year: millis.map(year_of),
        doi: None,
        url: Some(format!("https://openreview.net/forum?id={}", id)),
        pdf_url,
        venue: text("venue"),
        citations: None,
        fields,
        open_access: None,
        source: "openreview".to_string(),
        community: None,
    })
}

/// Most authors are plain strings; records imported from elsewhere carry
/// `{"fullname": …}`.
fn author_name(a: &serde_json::Value) -> Option<String> {
    a.as_str()
        .or_else(|| a["fullname"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Milliseconds since the epoch to a UTC calendar year, without a date crate
/// (Howard Hinnant's days-to-civil, year part only).
fn year_of(millis: i64) -> u16 {
    let z = millis.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SearchQuery;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openreview_search.json");

    fn papers() -> Vec<Paper> {
        parse_search_response(FIXTURE).unwrap()
    }

    #[test]
    fn parses_every_note() {
        assert_eq!(papers().len(), 6);
        assert!(papers().iter().all(|p| p.source == "openreview"));
    }

    // The decision is what dblp does not have.
    #[test]
    fn venue_carries_the_decision() {
        let venues: Vec<Option<String>> = papers().into_iter().map(|p| p.venue).collect();
        assert_eq!(venues[1].as_deref(), Some("Submitted to ICLR 2024"));
        assert_eq!(venues[5].as_deref(), Some("ICLR 2025 Spotlight"));
    }

    #[test]
    fn an_openreview_pdf_path_becomes_absolute() {
        assert_eq!(
            papers()[1].pdf_url.as_deref(),
            Some("https://openreview.net/pdf/eb64ab16c75a04aa0e9fbd0d256a2a61e6045fed.pdf")
        );
    }

    #[test]
    fn an_imported_record_keeps_its_external_pdf() {
        assert_eq!(
            papers()[2].pdf_url.as_deref(),
            Some("https://ieeexplore.ieee.org/iel7/65/10506038/10327705.pdf")
        );
        assert!(papers()[0].pdf_url.is_none());
    }

    #[test]
    fn authors_may_be_strings_or_fullname_objects() {
        assert_eq!(papers()[0].authors[0], "Sunil Sahu");
        assert_eq!(papers()[1].authors[0], "Beini Xie");
    }

    #[test]
    fn year_comes_from_pdate_then_cdate() {
        assert_eq!(papers()[0].year, Some(2019)); // pdate 1561939200000
        assert_eq!(papers()[1].year, Some(2023)); // no pdate; cdate 1695552920526
    }

    #[test]
    fn keywords_become_fields_and_the_forum_is_the_url() {
        let p = &papers()[1];
        assert_eq!(p.fields[0], "graph neural network");
        assert_eq!(
            p.url.as_deref(),
            Some("https://openreview.net/forum?id=IefMMX12yk")
        );
        assert!(p.abstract_text.is_some());
        assert!(p.doi.is_none() && p.citations.is_none());
    }

    #[test]
    fn a_challenge_body_is_an_error() {
        let body = r#"{"name":"ChallengeRequiredError","message":"Challenge verification required","status":403}"#;
        assert!(
            parse_search_response(body)
                .unwrap_err()
                .contains("ChallengeRequiredError")
        );
    }

    #[test]
    fn search_url_always_asks_for_forum_notes() {
        let url = build_search_url(
            "https://api2.openreview.net",
            &SearchQuery::simple("graph neural network", 5),
        );
        assert_eq!(
            url,
            "https://api2.openreview.net/notes/search?term=graph+neural+network&source=forum&limit=5"
        );
    }

    #[test]
    fn field_becomes_group_and_offset_passes_through() {
        let mut q = SearchQuery::simple("diffusion", 5);
        q.field = Some("ICLR.cc/2025/Conference".into());
        q.offset = 10;
        let url = build_search_url("https://api2.openreview.net", &q);
        assert!(url.contains("&offset=10"), "{}", url);
        assert!(
            url.contains("&group=ICLR.cc%2F2025%2FConference"),
            "{}",
            url
        );
    }
}
