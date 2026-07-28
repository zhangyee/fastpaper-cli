use scraper::{Html, Selector};

use super::Paper;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
];

/// Search Google Scholar (experimental, rate-limited).
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let ua_index = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as usize
        % USER_AGENTS.len();
    let ua = USER_AGENTS[ua_index];

    let url = build_search_url(base_url, q);

    match ureq::get(&url).header("User-Agent", ua).call() {
        Ok(resp) => {
            let body = resp
                .into_body()
                .read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            parse_search_response(&body)
        }
        Err(ureq::Error::StatusCode(429)) => {
            Err("Google Scholar rate limited (429). Try again later.".to_string())
        }
        Err(ureq::Error::StatusCode(403)) => {
            Err("Google Scholar blocked request (403). May need different IP.".to_string())
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Build the /scholar URL for a search.
///
/// `as_sdt=0,5` excludes patents. Google's switch only widens (7 mixes patents
/// in among the papers); it has no "patents only" setting, so it cannot back a
/// `--patents` flag that means what it says. For patents, use europepmc or
/// xueshu.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    format!(
        "{}/scholar?q={}&start={}&hl=en&as_sdt=0,5&num={}",
        base_url,
        super::encode_query(&q.query),
        q.offset,
        q.limit
    )
}

/// Parse Google Scholar HTML search results into a list of Papers.
pub fn parse_search_response(html: &str) -> Result<Vec<Paper>, String> {
    // Check for CAPTCHA
    if html.contains("gs_captcha_f") || html.contains("name=\"captcha\"") {
        return Err("Google Scholar CAPTCHA detected. Try again later.".to_string());
    }

    let document = Html::parse_document(html);
    // Iterate the row, not `gs_ri`: a row holds the bibliographic column
    // (`gs_ri`) *and*, when Scholar found free full text, a link block
    // (`gs_ggs`) beside it. Walking `gs_ri` alone cannot see the link.
    let row_sel = Selector::parse("div.gs_r").unwrap();
    let result_sel = Selector::parse("div.gs_ri").unwrap();
    let title_sel = Selector::parse("h3.gs_rt a").unwrap();
    let meta_sel = Selector::parse("div.gs_a").unwrap();
    let abstract_sel = Selector::parse("div.gs_rs").unwrap();
    let fulltext_sel = Selector::parse("div.gs_ggs a").unwrap();

    let mut papers = Vec::new();
    for row in document.select(&row_sel) {
        // The page ends with a `gs_r` block for the "create alert" control,
        // which has no bibliographic column; skipping those keeps it out.
        let Some(result) = row.select(&result_sel).next() else {
            continue;
        };

        // Scholar labels the link `[PDF]` for a file and `[HTML]` for a
        // publisher landing page. Only the first is downloadable.
        let pdf_url = row
            .select(&fulltext_sel)
            .find(|a| a.text().collect::<String>().contains("[PDF]"))
            .and_then(|a| a.value().attr("href"))
            .map(|s| s.to_string());

        // Title and URL
        let (title, url) = if let Some(link) = result.select(&title_sel).next() {
            let title = link.text().collect::<String>().trim().to_string();
            let url = link.value().attr("href").map(|s| s.to_string());
            (title, url)
        } else {
            continue;
        };

        if title.is_empty() {
            continue;
        }

        // Authors, year, venue from gs_a
        let mut authors = Vec::new();
        let mut year: Option<u16> = None;
        let mut venue: Option<String> = None;

        if let Some(meta_el) = result.select(&meta_sel).next() {
            let meta_text = meta_el.text().collect::<String>();
            // Format: "A Author, B Author - Journal Name, 2023 - publisher"
            // or: "A Author, B Author - 2023 - publisher"
            let parts: Vec<&str> = meta_text.split(" - ").collect();
            if !parts.is_empty() {
                authors = parts[0]
                    .split(',')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty() && !a.chars().all(|c| c == '…' || c.is_whitespace()))
                    .collect();
            }
            // Find year (4-digit number)
            for part in &parts {
                for word in part.split(|c: char| !c.is_ascii_digit()) {
                    if word.len() == 4 {
                        if let Ok(y) = word.parse::<u16>() {
                            if y >= 1900 && y <= 2100 {
                                year = Some(y);
                            }
                        }
                    }
                }
            }
            // Venue is typically the middle part
            if parts.len() >= 2 {
                let venue_part = parts[1].trim();
                // Remove year from venue
                let venue_clean = venue_part
                    .split(',')
                    .filter(|s| s.trim().parse::<u16>().is_err())
                    .collect::<Vec<&str>>()
                    .join(",")
                    .trim()
                    .to_string();
                if !venue_clean.is_empty() {
                    venue = Some(venue_clean);
                }
            }
        }

        // Abstract
        let abstract_text = result
            .select(&abstract_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let id = url
            .as_ref()
            .map(|u| u.split('/').last().unwrap_or(u).to_string())
            .unwrap_or_default();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi: None,
            url,
            pdf_url,
            venue,
            citations: None,
            fields: vec![],
            open_access: None,
            source: "scholar".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/scholar_search.html");

    fn url() -> String {
        let q = crate::sources::SearchQuery::simple("battery cathode", 10);
        build_search_url("https://scholar.google.com", &q)
    }

    #[test]
    fn patents_are_excluded() {
        assert!(url().contains("as_sdt=0,5"), "got: {}", url());
    }

    // `[CITATION]` results carry no <a> under h3.gs_rt — Scholar knows the work
    // only from other papers' reference lists. Emitting them with an empty URL
    // would put unfetchable entries into the download workflow, so they are
    // dropped. Characterisation test: guards behaviour that already holds.
    #[test]
    fn citation_stubs_are_dropped_rather_than_half_filled() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(papers.len() < FIXTURE.matches("gs_ri").count());
        for p in &papers {
            assert!(!p.title.is_empty(), "empty title survived: {:?}", p);
            assert!(p.url.is_some(), "linkless result survived: {}", p.title);
        }
    }

    // Scholar puts the free full-text link in a block beside the main column,
    // labelled [PDF] for a file or [HTML] for a publisher landing page. Only
    // the former is something `download` could ever use.
    #[test]
    fn a_pdf_link_becomes_the_pdf_url() {
        let p = by_title("Attention mechanism in neural networks: where");
        assert_eq!(
            p.pdf_url.as_deref(),
            Some("https://arxiv.org/pdf/2204.13154")
        );
    }

    #[test]
    fn an_html_link_is_not_a_pdf_url() {
        let p = by_title("Attention mechanisms and their");
        assert_eq!(p.pdf_url, None, "an [HTML] landing page is not a file");
    }

    #[test]
    fn a_hit_with_no_link_block_has_no_pdf_url() {
        let p = by_title("A review on the attention mechanism");
        assert_eq!(p.pdf_url, None);
    }

    // The link belongs to its own row; iterating the wrong container would
    // hand every hit the first link on the page.
    #[test]
    fn each_pdf_url_belongs_to_its_own_row() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let urls: Vec<_> = papers.iter().filter_map(|p| p.pdf_url.as_deref()).collect();
        let mut uniq = urls.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(
            urls.len(),
            uniq.len(),
            "a link was copied across rows: {urls:?}"
        );
        assert!(
            urls.len() >= 5,
            "expected several PDF links, got {}",
            urls.len()
        );
        assert!(urls.iter().all(|u| u.starts_with("http")), "got: {urls:?}");
    }

    fn by_title(needle: &str) -> Paper {
        parse_search_response(FIXTURE)
            .unwrap()
            .into_iter()
            .find(|p| p.title.contains(needle))
            .unwrap_or_else(|| panic!("fixture has no hit titled {needle}"))
    }

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_source_is_scholar() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "scholar");
        }
    }

    #[test]
    fn parse_title_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers.is_empty(), "should have results");
        for p in &papers {
            assert!(!p.title.is_empty(), "paper has empty title");
        }
    }

    #[test]
    fn parse_url_from_link() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_url: Vec<_> = papers.iter().filter(|p| p.url.is_some()).collect();
        assert!(!with_url.is_empty(), "no papers with url");
        for p in &with_url {
            assert!(p.url.as_ref().unwrap().starts_with("http"));
        }
    }

    #[test]
    fn parse_authors_year_venue() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers
            .iter()
            .filter(|p| p.abstract_text.is_some())
            .collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_captcha_returns_err() {
        let captcha_html =
            r#"<html><body><form id="gs_captcha_f"><input name="captcha"></form></body></html>"#;
        let result = parse_search_response(captcha_html);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CAPTCHA"));
    }

    #[test]
    fn parse_open_access_is_none() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.open_access.is_none(), "Scholar should not have OA info");
        }
    }

    #[test]
    fn search_returns_papers() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("attention mechanism", 10),
        )
        .unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_sends_user_agent() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("User-Agent", mockito::Matcher::Regex("Mozilla".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 10),
        );
        mock.assert();
    }

    #[test]
    fn search_request_path_contains_scholar() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/scholar".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 10),
        );
        mock.assert();
    }

    #[test]
    fn search_request_contains_query() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("q=test".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 10),
        );
        mock.assert();
    }

    #[test]
    fn search_request_contains_hl_en() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("hl=en".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 10),
        );
        mock.assert();
    }
}
