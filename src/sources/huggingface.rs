//! Hugging Face Papers — the AI community's daily paper list and its votes.
//!
//! What it adds is not coverage (every paper is an arXiv paper) but attention:
//! which papers people are upvoting on a day, in a week, in a month. The daily
//! lists come ranked by upvotes. `sort=trending` is Hugging Face's own rolling
//! order and ignores any date given with it (measured 2026-09-19: identical
//! with and without `month=`), so it is a separate listing, not a period.
//!
//! Every id is an arXiv id, so files come from arXiv, not from here.

use super::{Community, Listing, Paper};

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

/// `api/papers/search` answers at most this many and cannot page.
pub const KEYWORD_MAX: u32 = 120;

pub fn build_search_url(base_url: &str, query: &str, limit: u32) -> String {
    format!(
        "{}/api/papers/search?q={}&limit={}",
        base_url.trim_end_matches('/'),
        super::encode_query(query),
        limit
    )
}

/// One page of a listing. `page_size` must stay the same across pages,
/// because `p` counts pages of that size.
pub fn build_listing_url(
    base_url: &str,
    listing: &Listing,
    page_size: u32,
    page: u32,
) -> Result<String, String> {
    let selector = match listing {
        Listing::Trending => "sort=trending".to_string(),
        Listing::Day(day) => format!("date={}", day),
        Listing::Week(week) if week.ends_with("-W53") => {
            // Dec 28 always falls in the last ISO week of its year.
            return Err(format!(
                "Hugging Face's weekly list stops at W52, so {} has none.\n\
                 Ask for its days instead, e.g. --top {}-12-28",
                week,
                &week[..4]
            ));
        }
        Listing::Week(week) => format!("week={}", week),
        Listing::Month(month) => format!("month={}", month),
    };
    Ok(format!(
        "{}/api/daily_papers?{}&limit={}&p={}",
        base_url.trim_end_matches('/'),
        selector,
        page_size,
        page
    ))
}

/// `api/daily_papers` pages in blocks of at most this many.
const PAGE_MAX: u32 = 100;

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    match &q.listing {
        Some(listing) => listing_search(base_url, listing, q.limit),
        None => {
            if q.limit > KEYWORD_MAX {
                return Err(format!(
                    "huggingface keyword search returns at most {} results per request (asked for {})",
                    KEYWORD_MAX, q.limit
                ));
            }
            parse_list_response(&http_get(&build_search_url(base_url, &q.query, q.limit))?)
        }
    }
}

fn listing_search(base_url: &str, listing: &Listing, limit: u32) -> Result<Vec<Paper>, String> {
    let page_size = limit.clamp(1, PAGE_MAX);
    let mut papers = Vec::new();
    let mut page = 0;
    loop {
        let url = build_listing_url(base_url, listing, page_size, page)?;
        let (held, batch) = parse_list(&http_get(&url)?)?;
        papers.extend(batch);
        if (held as u32) < page_size || papers.len() as u32 >= limit {
            break;
        }
        page += 1;
    }
    papers.truncate(limit as usize);
    if papers.is_empty() && matches!(listing, Listing::Day(_)) {
        eprintln!(
            "[huggingface] no daily list for that day: Hugging Face publishes none on \
             weekends and some holidays, so try the nearest weekday."
        );
    }
    Ok(papers)
}

/// Fetch one paper by its arXiv id, with its linked model/dataset/Space counts.
pub fn get_by_id(base_url: &str, id: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/api/papers/{}",
        base_url.trim_end_matches('/'),
        super::encode_query(id.trim())
    );
    match http_get(&url) {
        Ok(body) => parse_paper_response(&body),
        Err(e) if e.contains("404") => Ok(None),
        Err(e) => Err(e),
    }
}

/// Anonymous callers get 500 API requests per 5 minutes per IP; `HF_TOKEN`
/// raises that. A 429 carries the seconds until the window resets.
fn http_get(url: &str) -> Result<String, String> {
    for attempt in 0..3u32 {
        let mut req = crate::http::api()
            .get(url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("User-Agent", USER_AGENT);
        if let Ok(token) = std::env::var("HF_TOKEN")
            && !token.trim().is_empty()
        {
            req = req.header("Authorization", &format!("Bearer {}", token.trim()));
        }
        let resp = req.call().map_err(|e| format!("HTTP error: {}", e))?;
        let status = resp.status().as_u16();
        let reset = reset_seconds(resp.headers().get("ratelimit").and_then(|v| v.to_str().ok()));
        let body = resp
            .into_body()
            .read_to_string()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        match status {
            200..=299 => return Ok(body),
            429 if attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_secs(reset.unwrap_or(5).min(60)));
            }
            429 => {
                return Err(format!(
                    "huggingface rate limited (429): anonymous callers get 500 API requests \
                     per 5 minutes per IP; set HF_TOKEN to raise it{}",
                    reset
                        .map(|s| format!(", or retry in {} s", s))
                        .unwrap_or_default()
                ));
            }
            404 => return Err("huggingface returned 404".to_string()),
            _ => {
                let reason = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v["error"].as_str().map(|s| s.trim().to_string()))
                    .unwrap_or_default();
                return Err(format!("huggingface returned HTTP {}: {}", status, reason)
                    .trim_end_matches([':', ' '])
                    .to_string());
            }
        }
    }
    Err("huggingface rate limited (429) after 3 attempts".to_string())
}

/// `RateLimit: "api";r=487;t=16` — `t` is the seconds until the window resets.
fn reset_seconds(header: Option<&str>) -> Option<u64> {
    header?
        .split(';')
        .find_map(|part| part.trim().strip_prefix("t="))
        .and_then(|t| t.parse().ok())
}

/// Parse `api/daily_papers` or `api/papers/search`: both answer an array of
/// `{paper: {...}, numComments, organization, ...}`.
pub fn parse_list_response(json: &str) -> Result<Vec<Paper>, String> {
    parse_list(json).map(|(_, papers)| papers)
}

/// Also returns how many items the page held before any were skipped, which
/// is what tells a short (last) page from a page with a malformed record.
pub(crate) fn parse_list(json: &str) -> Result<(usize, Vec<Paper>), String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    if let Some(msg) = root["error"].as_str() {
        return Err(format!("huggingface API error: {}", msg.trim()));
    }
    let items = root
        .as_array()
        .ok_or("expected a JSON array of papers from Hugging Face")?;

    let papers = items
        .iter()
        .filter_map(|item| {
            let mut paper = parse_paper(&item["paper"])?;
            let c = paper.community.get_or_insert_with(Community::default);
            c.comments = count(&item["numComments"]);
            if c.organization.is_none() {
                c.organization = item["organization"]["name"].as_str().map(str::to_string);
            }
            Some(paper)
        })
        .collect();
    Ok((items.len(), papers))
}

/// Parse `api/papers/<id>`: the paper object itself, plus the link counts
/// the lists leave out.
pub fn parse_paper_response(json: &str) -> Result<Option<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;
    if let Some(msg) = root["error"].as_str() {
        return Err(format!("huggingface API error: {}", msg.trim()));
    }
    let Some(mut paper) = parse_paper(&root) else {
        return Ok(None);
    };
    if let Some(c) = paper.community.as_mut() {
        c.linked_models = count(&root["numTotalModels"]);
        c.linked_datasets = count(&root["numTotalDatasets"]);
        c.linked_spaces = count(&root["numTotalSpaces"]);
    }
    Ok(Some(paper))
}

/// The fields a paper object carries wherever it appears.
fn parse_paper(p: &serde_json::Value) -> Option<Paper> {
    let id = p["id"].as_str()?.trim().to_string();
    let title = squash(p["title"].as_str().unwrap_or(""));
    if id.is_empty() || title.is_empty() {
        return None;
    }

    let authors = p["authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["name"].as_str())
                .map(squash)
                .filter(|n| !n.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let community = Community {
        upvotes: count(&p["upvotes"]),
        comments: None,
        organization: p["organization"]["name"].as_str().map(str::to_string),
        github_repo: p["githubRepo"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        github_stars: count(&p["githubStars"]),
        listed_on: p["submittedOnDailyAt"]
            .as_str()
            .and_then(|d| d.get(..10))
            .map(str::to_string),
        linked_models: None,
        linked_datasets: None,
        linked_spaces: None,
    };

    Some(Paper {
        id: id.clone(),
        title,
        authors,
        abstract_text: p["summary"].as_str().map(squash).filter(|s| !s.is_empty()),
        year: p["publishedAt"]
            .as_str()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok()),
        doi: None,
        url: Some(format!("https://huggingface.co/papers/{}", id)),
        // Every Hugging Face paper is an arXiv paper; the file lives there.
        pdf_url: Some(format!("https://arxiv.org/pdf/{}", id)),
        venue: None,
        citations: None,
        fields: vec![],
        open_access: Some(true),
        source: "huggingface".to_string(),
        community: Some(community),
    })
}

fn count(v: &serde_json::Value) -> Option<u32> {
    v.as_u64().map(|n| n as u32)
}

/// arXiv titles and abstracts arrive with their hard line breaks, which are
/// layout, not content.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAILY: &str = include_str!("../../tests/fixtures/huggingface_daily.json");
    const TRENDING: &str = include_str!("../../tests/fixtures/huggingface_trending.json");
    const SEARCH: &str = include_str!("../../tests/fixtures/huggingface_search.json");
    const PAPER: &str = include_str!("../../tests/fixtures/huggingface_paper.json");

    fn community(p: &Paper) -> &Community {
        p.community.as_ref().expect("huggingface always fills community")
    }

    #[test]
    fn a_daily_list_keeps_its_upvote_order() {
        let papers = parse_list_response(DAILY).unwrap();
        let ids: Vec<&str> = papers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["2608.09888", "2608.15089", "2608.14391"]);
        let votes: Vec<Option<u32>> = papers.iter().map(|p| community(p).upvotes).collect();
        assert_eq!(votes, [Some(781), Some(447), Some(283)]);
    }

    #[test]
    fn reads_every_community_signal_a_list_carries() {
        let c = community(&parse_list_response(DAILY).unwrap()[0]).clone();
        assert_eq!(c.comments, Some(6));
        assert_eq!(c.organization.as_deref(), Some("pathwaycom"));
        assert_eq!(
            c.github_repo.as_deref(),
            Some("https://github.com/pathwaycom/arc-task-gen")
        );
        assert_eq!(c.github_stars, Some(11184));
        assert_eq!(c.listed_on.as_deref(), Some("2026-08-11"));
        // Lists never carry the link counts; only `get` does.
        assert_eq!(c.linked_models, None);
    }

    #[test]
    fn a_paper_with_no_organization_says_so() {
        let papers = parse_list_response(DAILY).unwrap();
        assert_eq!(community(&papers[1]).organization, None);
    }

    #[test]
    fn maps_the_bibliographic_fields() {
        let p = &parse_list_response(DAILY).unwrap()[0];
        assert_eq!(p.source, "huggingface");
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.authors.len(), 9);
        assert_eq!(p.authors[0], "Björn Engdahl");
        assert!(p.abstract_text.as_deref().is_some_and(|a| a.len() > 100));
        assert_eq!(p.url.as_deref(), Some("https://huggingface.co/papers/2608.09888"));
        assert_eq!(p.pdf_url.as_deref(), Some("https://arxiv.org/pdf/2608.09888"));
        assert_eq!(p.open_access, Some(true));
        assert_eq!((p.doi.clone(), p.venue.clone(), p.citations), (None, None, None));
    }

    // A paper that is trending again years later was never on a daily list.
    #[test]
    fn an_old_trending_paper_has_no_listing_date() {
        let papers = parse_list_response(TRENDING).unwrap();
        assert_eq!(papers[0].id, "2412.20138");
        assert_eq!(community(&papers[0]).listed_on, None);
        assert_eq!(community(&papers[1]).listed_on.as_deref(), Some("2025-03-12"));
    }

    #[test]
    fn search_results_share_the_list_shape() {
        let papers = parse_list_response(SEARCH).unwrap();
        assert_eq!(papers[0].title, "Denoising Diffusion Probabilistic Models");
        assert_eq!(community(&papers[0]).upvotes, Some(9));
        assert_eq!(community(&papers[0]).comments, Some(5));
        assert_eq!(community(&papers[0]).github_repo, None);
    }

    #[test]
    fn a_single_paper_adds_its_link_counts() {
        let p = parse_paper_response(PAPER).unwrap().unwrap();
        assert_eq!(p.id, "2608.09888");
        let c = community(&p);
        assert_eq!(c.upvotes, Some(782));
        assert_eq!(
            (c.linked_models, c.linked_datasets, c.linked_spaces),
            (Some(0), Some(1), Some(0))
        );
    }

    // arXiv titles keep their hard line breaks: "…Sampling\n  in Around 10 Steps".
    #[test]
    fn line_breaks_inside_titles_are_layout_not_content() {
        let json = r#"[{"paper":{"id":"2206.00927","title":"DPM-Solver: A Fast ODE Solver\n  in Around 10 Steps"}}]"#;
        assert_eq!(
            parse_list_response(json).unwrap()[0].title,
            "DPM-Solver: A Fast ODE Solver in Around 10 Steps"
        );
    }

    #[test]
    fn an_api_error_is_an_error_not_an_empty_list() {
        let err = parse_list_response(r#"{"error":"✖ Too big: expected number to be <=100\n  → at limit"}"#)
            .unwrap_err();
        assert!(err.contains("Too big"), "got: {}", err);
    }

    #[test]
    fn an_empty_day_is_no_papers() {
        assert!(parse_list_response("[]").unwrap().is_empty());
    }

    #[test]
    fn a_record_without_an_id_is_skipped() {
        let json = r#"[{"paper":{"title":"No id"}},{"paper":{"id":"2608.00001","title":"Kept"}}]"#;
        assert_eq!(parse_list_response(json).unwrap().len(), 1);
    }

    #[test]
    fn search_url_carries_query_and_limit() {
        assert_eq!(
            build_search_url("https://huggingface.co", "diffusion models", 5),
            "https://huggingface.co/api/papers/search?q=diffusion+models&limit=5"
        );
    }

    #[test]
    fn listing_urls_page_by_number() {
        let base = "https://huggingface.co";
        assert_eq!(
            build_listing_url(base, &Listing::Month("2026-08".into()), 100, 1).unwrap(),
            "https://huggingface.co/api/daily_papers?month=2026-08&limit=100&p=1"
        );
        assert_eq!(
            build_listing_url(base, &Listing::Day("2026-09-18".into()), 10, 0).unwrap(),
            "https://huggingface.co/api/daily_papers?date=2026-09-18&limit=10&p=0"
        );
        assert_eq!(
            build_listing_url(base, &Listing::Week("2026-W38".into()), 10, 0).unwrap(),
            "https://huggingface.co/api/daily_papers?week=2026-W38&limit=10&p=0"
        );
        assert!(build_listing_url(base, &Listing::Trending, 10, 0).unwrap().contains("sort=trending"));
    }

    // The API's week pattern stops at W52 (measured: W53 -> 400).
    #[test]
    fn week_53_is_refused_before_asking() {
        let err = build_listing_url("https://huggingface.co", &Listing::Week("2026-W53".into()), 10, 0)
            .unwrap_err();
        assert!(err.contains("W52") && err.contains("--top 2026-12-28"), "got: {}", err);
    }

    #[test]
    fn the_reset_is_read_off_the_ratelimit_header() {
        assert_eq!(reset_seconds(Some(r#""api";r=487;t=16"#)), Some(16));
        assert_eq!(reset_seconds(Some(r#""api";r=0"#)), None);
        assert_eq!(reset_seconds(None), None);
    }

    #[test]
    fn keyword_search_refuses_more_than_the_endpoint_returns() {
        let q = crate::sources::SearchQuery::simple("diffusion", 121);
        let err = search("http://127.0.0.1:9", &q).unwrap_err();
        assert!(err.contains("at most 120"), "got: {}", err);
    }
}
