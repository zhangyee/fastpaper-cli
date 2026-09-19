//! J-STAGE — journals of Japanese academic societies (JST).
//!
//! The article search answers Atom XML. Per the official manual (J-STAGE WebAPI
//! ご利用マニュアル Ver.2.0, 2026-03-26): no abstracts are returned; `pubyear*`
//! resolve years only; `sortflg=2` needs a journal or ISSN (`ERR_014`), so there
//! is no usable sort. Zero hits is not an empty feed but status `ERR_001`, and
//! `WARN_002` only says there were more hits than one page holds.
//!
//! Titles, authors and journal follow the record's Japanese when it has a
//! Japanese title, English otherwise (Yee, 2026-09-19).

use quick_xml::Reader;
use quick_xml::events::Event;

use super::Paper;

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

pub const RATE_LIMITED: &str = "jstage refused the request: too many concurrent requests (ERR_003)";

fn build_search_url(base_url: &str, q: &super::SearchQuery) -> String {
    let mut url = format!(
        "{}/searchapi/do?service=3&text={}&count={}",
        base_url.trim_end_matches('/'),
        super::encode_query(&q.query),
        q.limit
    );
    if q.offset > 0 {
        url.push_str(&format!("&start={}", q.offset + 1));
    }
    if let Some(ref author) = q.author {
        url.push_str(&format!("&author={}", super::encode_query(author)));
    }
    if let Some(year) = q.year {
        url.push_str(&format!("&pubyearfrom={y}&pubyearto={y}", y = year));
    }
    url
}

pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q);
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(u64::from(attempt)));
        }
        match parse_search_response(&http_get(&url)?) {
            Err(e) if e == RATE_LIMITED => continue,
            other => return other,
        }
    }
    Err(format!("{} after 3 attempts", RATE_LIMITED))
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
    if !(200..300).contains(&status) {
        return Err(format!("jstage returned HTTP {}", status));
    }
    Ok(body)
}

#[derive(Default)]
struct Entry {
    title_en: String,
    title_ja: String,
    link_en: String,
    link_ja: String,
    authors_en: Vec<String>,
    authors_ja: Vec<String>,
    venue_en: String,
    venue_ja: String,
    doi: String,
    pubyear: String,
}

pub fn parse_search_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let (mut status, mut message) = (String::new(), String::new());
    let mut entry: Option<Entry> = None;
    let mut papers = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = local(e.name().as_ref());
                if name == "entry" {
                    entry = Some(Entry::default());
                }
                path.push(name);
                text.clear();
            }
            Ok(Event::Text(e)) => text.push_str(&e.decode().unwrap_or_default()),
            Ok(Event::CData(e)) => text.push_str(&unescape(&String::from_utf8_lossy(&e.into_inner()))),
            Ok(Event::GeneralRef(e)) => {
                if let Ok(Some(c)) = e.resolve_char_ref() {
                    text.push(c);
                } else if let Ok(name) = e.decode() {
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Ok(Event::End(_)) => {
                let value = text.trim().to_string();
                text.clear();
                let joined = path.join("/");
                match joined.as_str() {
                    "feed/result/status" => status = value,
                    "feed/result/message" => message = value,
                    "feed/entry" => {
                        if let Some(done) = entry.take()
                            && let Some(paper) = to_paper(done)
                        {
                            papers.push(paper);
                        }
                    }
                    other => {
                        if let Some(ref mut e) = entry {
                            record(e, other, value);
                        }
                    }
                }
                path.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    match status.as_str() {
        "" | "0" | "WARN_002" => Ok(papers),
        "ERR_001" => Ok(Vec::new()),
        "ERR_003" => Err(RATE_LIMITED.to_string()),
        code => Err(format!("jstage API error {}: {}", code, message)),
    }
}

fn record(e: &mut Entry, path: &str, value: String) {
    if value.is_empty() {
        return;
    }
    match path.strip_prefix("feed/entry/").unwrap_or("") {
        "article_title/en" => e.title_en = value,
        "article_title/ja" => e.title_ja = value,
        "article_link/en" => e.link_en = value,
        "article_link/ja" => e.link_ja = value,
        "author/en/name" => e.authors_en.push(value),
        "author/ja/name" => e.authors_ja.push(value),
        "material_title/en" => e.venue_en = value,
        "material_title/ja" => e.venue_ja = value,
        "doi" => e.doi = value,
        "pubyear" => e.pubyear = value,
        _ => {}
    }
}

fn to_paper(e: Entry) -> Option<Paper> {
    let ja = !e.title_ja.is_empty();
    let title = prefer(ja, e.title_ja, e.title_en);
    if title.is_empty() {
        return None;
    }
    let link = if e.link_en.is_empty() { &e.link_ja } else { &e.link_en };
    let (prefix, id) = article_path(link)?;
    let pdf_url = format!("{}{}/_pdf", prefix, id);
    let authors = if (ja && !e.authors_ja.is_empty()) || e.authors_en.is_empty() {
        e.authors_ja
    } else {
        e.authors_en
    };
    let url = prefer(ja, e.link_ja.clone(), e.link_en.clone());
    let venue = prefer(ja, e.venue_ja, e.venue_en);

    Some(Paper {
        id: id.to_string(),
        title,
        authors,
        abstract_text: None,
        year: e.pubyear.get(..4).and_then(|y| y.parse().ok()),
        doi: Some(e.doi).filter(|d| !d.is_empty()),
        url: Some(url).filter(|u| !u.is_empty()),
        pdf_url: Some(pdf_url),
        venue: Some(venue).filter(|v| !v.is_empty()),
        citations: None,
        fields: vec![],
        open_access: None,
        source: "jstage".to_string(),
        community: None,
    })
}

/// The first non-empty of the preferred language and the other one.
fn prefer(ja: bool, ja_value: String, en_value: String) -> String {
    let (first, second) = if ja { (ja_value, en_value) } else { (en_value, ja_value) };
    if first.is_empty() { second } else { first }
}

/// `https://www.jstage.jst.go.jp/article/faruawpsj/54/9/54_843/_article` ->
/// (`https://www.jstage.jst.go.jp/article/`, `faruawpsj/54/9/54_843`).
fn article_path(link: &str) -> Option<(&str, &str)> {
    let start = link.find("/article/")? + "/article/".len();
    let end = start + link[start..].find("/_article")?;
    Some((&link[..start], &link[start..end]))
}

/// J-STAGE escapes inside CDATA, where escapes are literal text.
fn unescape(s: &str) -> String {
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SearchQuery;

    const JA: &str = include_str!("../../tests/fixtures/jstage_search.xml");
    const EN: &str = include_str!("../../tests/fixtures/jstage_english.xml");

    fn wrap(status: &str, entries: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><feed xmlns="http://www.w3.org/2005/Atom"><result><status>{}</status><message>{}</message></result>{}</feed>"#,
            status, status, entries
        )
    }

    #[test]
    fn warn_002_is_a_normal_page() {
        assert_eq!(parse_search_response(JA).unwrap().len(), 3);
    }

    // Titles follow the record's Japanese when it has one (Yee, 2026-09-19).
    #[test]
    fn a_japanese_title_wins_and_the_rest_follows_it() {
        let p = &parse_search_response(JA).unwrap()[0];
        assert_eq!(p.title, "AI医療の可能性と課題");
        assert_eq!(p.authors, vec!["大田 信行"]);
        assert_eq!(p.venue.as_deref(), Some("ファルマシア"));
        assert_eq!(
            p.url.as_deref(),
            Some("https://www.jstage.jst.go.jp/article/faruawpsj/54/9/54_843/_article/-char/ja/")
        );
    }

    #[test]
    fn english_is_the_fallback() {
        let p = &parse_search_response(EN).unwrap()[0];
        assert!(p.title.starts_with("Liquid-Phase Oxidation"), "{}", p.title);
        assert_eq!(p.authors[0], "MUHAMMADISHAQALI KHAN");
    }

    #[test]
    fn id_is_the_article_path_and_the_pdf_sits_beside_it() {
        let p = &parse_search_response(JA).unwrap()[0];
        assert_eq!(p.id, "faruawpsj/54/9/54_843");
        assert_eq!(
            p.pdf_url.as_deref(),
            Some("https://www.jstage.jst.go.jp/article/faruawpsj/54/9/54_843/_pdf")
        );
        assert_eq!(p.doi.as_deref(), Some("10.14894/faruawpsj.54.9_843"));
        assert_eq!(p.year, Some(2018));
        assert_eq!(p.source, "jstage");
    }

    #[test]
    fn no_abstracts_and_no_access_flag() {
        for p in parse_search_response(JA).unwrap() {
            assert!(p.abstract_text.is_none() && p.open_access.is_none());
        }
    }

    // J-STAGE escapes inside CDATA, where escapes are not supposed to apply.
    #[test]
    fn escapes_inside_cdata_are_undone() {
        let entry = "<entry><article_title><en><![CDATA[I&apos;ve heard]]></en></article_title>\
                     <article_link><en>https://www.jstage.jst.go.jp/article/x/1/1/1_1/_article</en></article_link></entry>";
        assert_eq!(parse_search_response(&wrap("0", entry)).unwrap()[0].title, "I've heard");
    }

    #[test]
    fn err_001_means_no_results_not_an_error() {
        assert!(parse_search_response(&wrap("ERR_001", "")).unwrap().is_empty());
    }

    #[test]
    fn err_003_is_the_rate_limit() {
        assert_eq!(parse_search_response(&wrap("ERR_003", "")).unwrap_err(), RATE_LIMITED);
    }

    #[test]
    fn other_codes_are_errors_that_name_the_code() {
        assert!(parse_search_response(&wrap("ERR_004", "")).unwrap_err().contains("ERR_004"));
    }

    #[test]
    fn search_url_maps_every_supported_filter() {
        let mut q = SearchQuery::simple("深層学習 医療", 20);
        q.offset = 20;
        q.author = Some("Kondo".into());
        q.year = Some(2024);
        let url = build_search_url("https://api.jstage.jst.go.jp", &q);
        assert!(url.starts_with("https://api.jstage.jst.go.jp/searchapi/do?service=3&text="), "{}", url);
        assert!(url.contains("&count=20&start=21"), "{}", url);
        assert!(url.contains("&author=Kondo"), "{}", url);
        assert!(url.contains("&pubyearfrom=2024&pubyearto=2024"), "{}", url);
    }
}
