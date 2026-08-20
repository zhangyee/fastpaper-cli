use quick_xml::Reader;
use quick_xml::events::Event;

use crate::download::FetchError;

use super::Paper;

/// Download PDF bytes from arXiv.
///
/// Delegates the fetch so that the size limit lives in one place; the only
/// thing arXiv adds is being able to name the paper in a 404.
pub fn download_pdf(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<u8>, crate::download::FetchError> {
    let url = format!("{}/pdf/{}.pdf", base_url, identifier);
    let not_found = format!("Paper not found: {}", identifier);
    crate::download::fetch_pdf_named(&url, limit, &not_found)
}

/// Fetch the paper's e-print source package and hand back its figure files.
///
/// `base_url` is the *PDF* base (`https://arxiv.org`), not the API host: the
/// e-print endpoint lives with the files, not with the Atom API.
pub fn figures(
    base_url: &str,
    identifier: &str,
    limit: u64,
) -> Result<Vec<(String, Vec<u8>)>, FetchError> {
    let url = format!("{}/e-print/{}", base_url.trim_end_matches('/'), identifier);
    let not_found = format!("Not found on arXiv: {}", identifier);
    let bytes = crate::download::fetch_archive(&url, limit, &not_found)?;

    // A PDF body means a PDF-only submission: there is no source package, so
    // there are no original figure files to hand back. Said plainly here
    // rather than letting the gzip reader call it a corrupt archive.
    if bytes.starts_with(b"%PDF") {
        return Err(FetchError::NotFound(format!(
            "arXiv has no source package for {} (it was submitted as a PDF), so there are no original figure files.\nTo get the paper itself: fastpaper download arxiv {}",
            identifier, identifier
        )));
    }

    crate::figures::untar_gz_images(&bytes)
}

/// Fetch a single paper by arXiv ID.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/api/query?id_list={}&max_results=1",
        base_url, identifier
    );
    let body = http_get_with_retry(&url)?;
    let papers = parse_search_response(&body)?;
    Ok(papers.into_iter().next())
}

/// GET with backoff on 429.
///
/// arXiv asks callers to leave three seconds between requests and throttles
/// hard when they don't. `search` has always retried; `get_by_id` did not, so a
/// single 429 failed the whole lookup.
fn http_get_with_retry(url: &str) -> Result<String, String> {
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
                last_err = "rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("Server error: {}", code));
            }
            Err(e) => return Err(format!("HTTP error: {}", e)),
        }
    }
    Err(last_err)
}

/// Turn `YYYY-MM-DD` into arXiv's `YYYYMMDDHHMM` stamp.
fn date_stamp(date: &str, end_of_day: bool) -> Result<String, String> {
    let digits: String = super::validate_ymd(date)?.replace('-', "");
    let time = if end_of_day { "2359" } else { "0000" };
    Ok(format!("{}{}", digits, time))
}

/// Turn the user's keywords into one `search_query` term.
///
/// Every word needs its own `all:` prefix. A bare multi-word query parses as a
/// field-prefixed first word followed by loose words, and the moment anything
/// is ANDed onto it the whole expression stops filtering: `all:diffusion model
/// AND submittedDate:[2025...]` returns 2011 papers. Grouping the words keeps
/// the AND applying to the right thing.
fn query_term(query: &str) -> String {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| format!("all:{}", super::encode_query(w)))
        .collect();
    match words.len() {
        0 => "all:".to_string(),
        1 => words.into_iter().next().unwrap(),
        _ => format!("%28{}%29", words.join("+AND+")),
    }
}

/// Build the /api/query URL for a search.
///
/// arXiv composes filters into `search_query` itself, ANDing field-prefixed
/// terms (`au:`, `cat:`, `submittedDate:`) rather than taking separate
/// parameters. Field prefixes and operators stay literal; only values are
/// percent-encoded.
fn build_search_url(base_url: &str, q: &super::SearchQuery) -> Result<String, String> {
    let mut terms = vec![query_term(&q.query)];

    if let Some(ref author) = q.author {
        terms.push(format!("au:%22{}%22", super::encode_query(author)));
    }
    if let Some(ref field) = q.field {
        terms.push(format!("cat:{}", super::encode_query(field)));
    }

    // --year is shorthand for a whole-year range; --after/--before give the
    // open-ended forms.
    let range = match (q.year, q.after.as_deref(), q.before.as_deref()) {
        (Some(year), _, _) => Some((format!("{}01010000", year), format!("{}12312359", year))),
        (None, None, None) => None,
        (None, after, before) => Some((
            match after {
                Some(d) => date_stamp(d, false)?,
                None => "190001010000".to_string(),
            },
            match before {
                Some(d) => date_stamp(d, true)?,
                None => "299912312359".to_string(),
            },
        )),
    };
    if let Some((from, to)) = range {
        terms.push(format!("submittedDate:%5B{}+TO+{}%5D", from, to));
    }

    // Every arXiv paper is freely readable, so --open-access is always
    // satisfied and contributes no term.

    let mut url = format!(
        "{}/api/query?search_query={}&start={}&max_results={}",
        base_url,
        terms.join("+AND+"),
        q.offset,
        q.limit
    );

    if let Some(sort) = q.sort {
        let by = match sort {
            super::SortField::Relevance => "relevance",
            super::SortField::Date => "submittedDate",
            super::SortField::Citations => {
                return Err(
                    "arxiv cannot sort by citations: it publishes no citation counts.\n\
                     Try --sort date, or use semantic or openalex for citation ordering."
                        .to_string(),
                );
            }
        };
        let order = match q.order {
            super::SortOrder::Asc => "ascending",
            super::SortOrder::Desc => "descending",
        };
        url.push_str(&format!("&sortBy={}&sortOrder={}", by, order));
    }

    Ok(url)
}

/// Search arXiv API and return parsed papers.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let url = build_search_url(base_url, q)?;
    let body = http_get_with_retry(&url)?;
    parse_search_response(&body)
}

/// Parse arXiv Atom/XML search response into a list of Papers.
pub fn parse_search_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut papers = Vec::new();
    let mut buf = Vec::new();

    // State for current entry being parsed
    let mut in_entry = false;
    let mut current_tag = String::new();
    let mut id = String::new();
    let mut title = String::new();
    let mut summary = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut published = String::new();
    let mut pdf_url: Option<String> = None;
    let mut doi: Option<String> = None;
    let mut fields: Vec<String> = Vec::new();
    let mut in_author = false;
    let mut author_name = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "entry" => {
                        in_entry = true;
                        id.clear();
                        title.clear();
                        summary.clear();
                        authors.clear();
                        published.clear();
                        pdf_url = None;
                        doi = None;
                        fields.clear();
                    }
                    "author" if in_entry => {
                        in_author = true;
                        author_name.clear();
                    }
                    "link" if in_entry => {
                        let mut href = None;
                        let mut link_type = None;
                        let mut link_title = None;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"href" => {
                                    href = Some(String::from_utf8_lossy(&attr.value).to_string())
                                }
                                b"type" => {
                                    link_type =
                                        Some(String::from_utf8_lossy(&attr.value).to_string())
                                }
                                b"title" => {
                                    link_title =
                                        Some(String::from_utf8_lossy(&attr.value).to_string())
                                }
                                _ => {}
                            }
                        }
                        if link_type.as_deref() == Some("application/pdf") {
                            pdf_url = href.clone();
                        }
                        if link_title.as_deref() == Some("doi") {
                            if let Some(ref h) = href {
                                // Extract DOI from doi.org URL
                                doi = doi.or_else(|| {
                                    h.strip_prefix("https://doi.org/")
                                        .or_else(|| h.strip_prefix("http://doi.org/"))
                                        .map(|s| s.to_string())
                                });
                            }
                        }
                    }
                    "category" if in_entry => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"term" {
                                fields.push(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "doi" if in_entry => {
                        current_tag = "doi".to_string();
                    }
                    _ if in_entry => {
                        current_tag = local.to_string();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_entry {
                    let text = e.decode().unwrap_or_default().to_string();
                    if in_author && current_tag == "name" {
                        author_name.push_str(&text);
                    } else {
                        match current_tag.as_str() {
                            "id" => id.push_str(&text),
                            "title" => title.push_str(&text),
                            "summary" => summary.push_str(&text),
                            "published" => published.push_str(&text),
                            "doi" => {
                                doi = Some(text);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "entry" => {
                        // Extract arXiv ID from URL: last path segment, strip version
                        let arxiv_id = id.rsplit('/').next().unwrap_or(&id).to_string();
                        // Strip version suffix for the id
                        let clean_id = if let Some(pos) = arxiv_id.rfind('v') {
                            if arxiv_id[pos + 1..].chars().all(|c| c.is_ascii_digit()) {
                                arxiv_id[..pos].to_string()
                            } else {
                                arxiv_id.clone()
                            }
                        } else {
                            arxiv_id.clone()
                        };

                        let year = published.get(..4).and_then(|y| y.parse::<u16>().ok());

                        papers.push(Paper {
                            id: clean_id,
                            title: title.trim().to_string(),
                            authors: authors.clone(),
                            abstract_text: if summary.trim().is_empty() {
                                None
                            } else {
                                Some(summary.trim().to_string())
                            },
                            year,
                            doi: doi.clone(),
                            url: Some(id.clone()),
                            pdf_url: pdf_url.clone(),
                            venue: Some("arXiv preprint".to_string()),
                            citations: None,
                            fields: fields.clone(),
                            open_access: Some(true),
                            source: "arxiv".to_string(),
                        });
                        in_entry = false;
                    }
                    "author" if in_entry => {
                        if !author_name.trim().is_empty() {
                            authors.push(author_name.trim().to_string());
                        }
                        in_author = false;
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

/// Extract local name from a possibly namespaced XML tag.
fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/arxiv_search.xml");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_papers_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert!(!papers.is_empty());
    }

    #[test]
    fn parse_titles_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_source_is_arxiv() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "arxiv");
        }
    }

    #[test]
    fn parse_has_authors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.authors.is_empty(), "paper {} has no authors", p.id);
        }
    }

    #[test]
    fn parse_url_contains_arxiv() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            let url = p.url.as_ref().expect("paper should have url");
            assert!(
                url.contains("arxiv.org"),
                "url {} doesn't contain arxiv.org",
                url
            );
        }
    }

    #[test]
    fn parse_id_is_arxiv_format() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(
                p.id.contains('.') || p.id.contains('/'),
                "id {} doesn't look like arXiv ID",
                p.id
            );
        }
    }

    #[test]
    fn parse_empty_feed_returns_empty_list() {
        let papers = parse_search_response("<feed></feed>").unwrap();
        assert!(papers.is_empty());
    }

    #[test]
    fn parse_doi_not_empty_when_present() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        for p in &with_doi {
            assert!(
                !p.doi.as_ref().unwrap().is_empty(),
                "paper {} has empty doi",
                p.id
            );
        }
    }

    #[test]
    fn parse_abstract_not_none_when_present() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // Our fixture has summaries for all entries
        for p in &papers {
            assert!(p.abstract_text.is_some(), "paper {} missing abstract", p.id);
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
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();
        assert!(!papers.is_empty());
        mock.assert();
    }

    #[test]
    fn search_returns_correct_count() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        )
        .unwrap();
        assert_eq!(papers.len(), 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_search_query() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("search_query".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("attention", 3),
        );
        mock.assert();
    }

    #[test]
    fn search_request_contains_max_results() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("max_results=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        mock.assert();
    }

    #[test]
    fn search_500_returns_err() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(500)
            .create();
        let result = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
        mock.assert();
    }

    #[test]
    fn search_429_retries() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .expect_at_least(2)
            .create();
        let result = search(
            &server.url(),
            &crate::sources::SearchQuery::simple("test", 3),
        );
        assert!(result.is_err());
        mock.assert();
    }

    #[test]
    fn get_by_id_returns_paper() {
        let mut server = mockito::Server::new();
        server
            .mock(
                "GET",
                mockito::Matcher::Regex("id_list=2301.08745".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = get_by_id(&server.url(), "2301.08745");
        assert!(result.is_ok());
        let paper = result.unwrap();
        assert!(paper.is_some());
    }

    #[test]
    fn get_by_id_has_title() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let paper = get_by_id(&server.url(), "2301.08745").unwrap().unwrap();
        assert!(!paper.title.is_empty());
    }

    #[test]
    fn get_by_id_has_authors() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let paper = get_by_id(&server.url(), "2301.08745").unwrap().unwrap();
        assert!(!paper.authors.is_empty());
    }

    #[test]
    fn get_by_id_empty_feed_returns_none() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body("<feed></feed>")
            .create();
        let result = get_by_id(&server.url(), "9999.99999").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn download_pdf_returns_bytes() {
        let fake_pdf = b"%PDF-1.4 fake content";
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(fake_pdf.as_slice())
            .create();
        let bytes = download_pdf(&server.url(), "2301.08745", 10 * 1024 * 1024).unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF"));
        mock.assert();
    }

    #[test]
    fn download_pdf_404_returns_err() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .create();
        let result = download_pdf(&server.url(), "9999.99999", 10 * 1024 * 1024);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("not found"));
    }

    // arXiv answers `e-print` with the paper's own PDF when the submission had
    // no TeX source (2405.09567 is one). Unpacking that as a tarball would
    // report a corrupt archive; the truth is that there are no source figures.
    #[test]
    fn a_pdf_only_submission_reports_no_source_package() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/e-print/2405.09567")
            .with_status(200)
            .with_body(b"%PDF-1.7 the paper itself".as_slice())
            .create();

        let err = figures(&server.url(), "2405.09567", 100 * 1024 * 1024).unwrap_err();
        assert!(matches!(err, FetchError::NotFound(_)));
        assert!(
            err.message().contains("no source package"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn a_source_tarball_yields_its_figure_files() {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, body) in [("1.pdf", &b"fig-one"[..]), ("main.tex", &b"latex"[..])] {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, body).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut enc =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, &raw).unwrap();
        let tgz = enc.finish().unwrap();

        let mut server = mockito::Server::new();
        server
            .mock("GET", "/e-print/2511.11035")
            .with_status(200)
            .with_body(tgz)
            .create();

        let got = figures(&server.url(), "2511.11035", 100 * 1024 * 1024).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "1.pdf");
        assert_eq!(got[0].1, b"fig-one");
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::sources::{SearchQuery, SortField, SortOrder};

    fn url(q: &SearchQuery) -> String {
        build_search_url("https://export.arxiv.org", q).unwrap()
    }

    #[test]
    fn plain_query_searches_all_fields() {
        let u = url(&SearchQuery::simple("attention", 10));
        assert!(u.contains("search_query=all:attention"), "got: {}", u);
        assert!(u.contains("max_results=10"), "got: {}", u);
        assert!(u.contains("start=0"), "got: {}", u);
    }

    // A bare multi-word query breaks the boolean structure once anything is
    // ANDed onto it: arXiv reads `all:diffusion model AND submittedDate:[...]`
    // as something that ignores the date range entirely, returning 2011 papers
    // for a 2025 filter. Each word needs its own field prefix.
    #[test]
    fn multi_word_query_is_grouped_per_term() {
        let u = url(&SearchQuery::simple("diffusion model", 10));
        assert!(u.contains("all:diffusion+AND+all:model"), "got: {}", u);
        assert!(
            !u.contains("all:diffusion+model"),
            "bare form breaks filters: {}",
            u
        );
    }

    #[test]
    fn multi_word_query_is_parenthesised_when_a_filter_follows() {
        let mut q = SearchQuery::simple("diffusion model", 10);
        q.year = Some(2025);
        let u = url(&q);
        assert!(
            u.contains("%28all:diffusion+AND+all:model%29"),
            "got: {}",
            u
        );
        assert!(u.contains("submittedDate:"), "got: {}", u);
    }

    #[test]
    fn single_word_query_is_left_alone() {
        let u = url(&SearchQuery::simple("attention", 10));
        assert!(u.contains("search_query=all:attention"), "got: {}", u);
        assert!(!u.contains("%28"), "no grouping needed: {}", u);
    }

    #[test]
    fn offset_becomes_start() {
        let mut q = SearchQuery::simple("attention", 10);
        q.offset = 30;
        assert!(url(&q).contains("start=30"));
    }

    #[test]
    fn author_is_anded_onto_the_query() {
        let mut q = SearchQuery::simple("attention", 10);
        q.author = Some("Vaswani".into());
        let u = url(&q);
        assert!(u.contains("AND"), "terms should be ANDed: {}", u);
        assert!(u.contains("au:"), "got: {}", u);
        assert!(u.contains("Vaswani"), "got: {}", u);
    }

    #[test]
    fn field_becomes_a_category_term() {
        let mut q = SearchQuery::simple("attention", 10);
        q.field = Some("cs.CL".into());
        assert!(url(&q).contains("cat:cs.CL"), "got: {}", url(&q));
    }

    // arXiv expects submittedDate:[YYYYMMDDTTTT TO YYYYMMDDTTTT].
    #[test]
    fn year_becomes_a_submitted_date_range() {
        let mut q = SearchQuery::simple("attention", 10);
        q.year = Some(2024);
        let u = url(&q);
        assert!(u.contains("submittedDate:"), "got: {}", u);
        assert!(u.contains("202401010000"), "got: {}", u);
        assert!(u.contains("202412312359"), "got: {}", u);
    }

    #[test]
    fn after_and_before_become_a_submitted_date_range() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2023-06-01".into());
        q.before = Some("2023-12-31".into());
        let u = url(&q);
        assert!(u.contains("202306010000"), "got: {}", u);
        assert!(u.contains("202312312359"), "got: {}", u);
    }

    #[test]
    fn after_alone_runs_to_an_open_upper_bound() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("2023-06-01".into());
        let u = url(&q);
        assert!(u.contains("202306010000"), "got: {}", u);
        assert!(u.contains("TO"), "got: {}", u);
    }

    #[test]
    fn malformed_date_is_rejected() {
        let mut q = SearchQuery::simple("attention", 10);
        q.after = Some("June 2023".into());
        let err = build_search_url("https://export.arxiv.org", &q).unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "got: {}", err);
    }

    #[test]
    fn sort_by_date_maps_to_submitted_date() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Date);
        let u = url(&q);
        assert!(u.contains("sortBy=submittedDate"), "got: {}", u);
        assert!(u.contains("sortOrder=descending"), "got: {}", u);
    }

    #[test]
    fn sort_order_ascending_is_honoured() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Date);
        q.order = SortOrder::Asc;
        assert!(url(&q).contains("sortOrder=ascending"));
    }

    #[test]
    fn sort_by_relevance_is_the_api_default_name() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Relevance);
        assert!(url(&q).contains("sortBy=relevance"));
    }

    // arXiv exposes no citation counts, so it cannot sort by them.
    #[test]
    fn sort_by_citations_is_rejected_by_name() {
        let mut q = SearchQuery::simple("attention", 10);
        q.sort = Some(SortField::Citations);
        let err = build_search_url("https://export.arxiv.org", &q).unwrap_err();
        assert!(err.contains("citations"), "got: {}", err);
        assert!(err.contains("arxiv"), "got: {}", err);
    }

    #[test]
    fn no_sort_flag_leaves_sort_params_off() {
        let u = url(&SearchQuery::simple("attention", 10));
        assert!(!u.contains("sortBy"), "got: {}", u);
    }

    // Everything on arXiv is free to read, so --open-access is always
    // satisfied and adds no term.
    #[test]
    fn open_access_adds_no_term_because_arxiv_is_all_open_access() {
        let mut q = SearchQuery::simple("attention", 10);
        q.open_access = true;
        assert_eq!(url(&q), url(&SearchQuery::simple("attention", 10)));
    }
}

#[cfg(test)]
mod get_retry_tests {
    use super::*;

    // search has always backed off on 429; get_by_id used to fail outright.
    #[test]
    fn get_by_id_retries_after_a_429() {
        let fixture = include_str!("../../tests/fixtures/arxiv_search.xml");
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .expect(1)
            .create();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(fixture)
            .create();
        let paper = get_by_id(&server.url(), "2301.08745").unwrap();
        assert!(paper.is_some(), "should succeed on the retry");
    }

    #[test]
    fn get_by_id_gives_up_after_repeated_429s() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .create();
        let err = get_by_id(&server.url(), "2301.08745").unwrap_err();
        assert!(err.contains("429"), "got: {}", err);
    }
}
