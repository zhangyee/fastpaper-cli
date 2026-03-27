use quick_xml::events::Event;
use quick_xml::Reader;

use super::Paper;

/// Download PDF bytes from arXiv.
pub fn download_pdf(base_url: &str, identifier: &str) -> Result<Vec<u8>, String> {
    let url = format!("{}/pdf/{}.pdf", base_url, identifier);
    match ureq::get(&url).call() {
        Ok(resp) => {
            let bytes = resp
                .into_body()
                .read_to_vec()
                .map_err(|e| format!("Failed to read PDF: {}", e))?;
            Ok(bytes)
        }
        Err(ureq::Error::StatusCode(404)) => {
            Err(format!("Paper not found: {}", identifier))
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}



/// Fetch a single paper by arXiv ID.
pub fn get_by_id(base_url: &str, identifier: &str) -> Result<Option<Paper>, String> {
    let url = format!(
        "{}/api/query?id_list={}&max_results=1",
        base_url, identifier
    );
    match ureq::get(&url).call() {
        Ok(resp) => {
            let body = resp
                .into_body()
                .read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            let papers = parse_search_response(&body)?;
            Ok(papers.into_iter().next())
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Search arXiv API and return parsed papers.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let url = format!(
        "{}/api/query?search_query=all:{}&start=0&max_results={}",
        base_url, query, max_results
    );

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        let response = ureq::get(&url).call();
        match response {
            Ok(resp) => {
                let body = resp
                    .into_body()
                    .read_to_string()
                    .map_err(|e| format!("Failed to read response: {}", e))?;
                return parse_search_response(&body);
            }
            Err(ureq::Error::StatusCode(429)) => {
                last_err = "rate limited (429)".to_string();
                continue;
            }
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                return Err(format!("Server error: {}", code));
            }
            Err(e) => {
                return Err(format!("HTTP error: {}", e));
            }
        }
    }
    Err(last_err)
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
                                b"href" => href = Some(String::from_utf8_lossy(&attr.value).to_string()),
                                b"type" => link_type = Some(String::from_utf8_lossy(&attr.value).to_string()),
                                b"title" => link_title = Some(String::from_utf8_lossy(&attr.value).to_string()),
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
                        let arxiv_id = id
                            .rsplit('/')
                            .next()
                            .unwrap_or(&id)
                            .to_string();
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
            assert!(url.contains("arxiv.org"), "url {} doesn't contain arxiv.org", url);
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
            assert!(!p.doi.as_ref().unwrap().is_empty(), "paper {} has empty doi", p.id);
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
        let papers = search(&server.url(), "test", 3).unwrap();
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
        let papers = search(&server.url(), "test", 3).unwrap();
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
        let _ = search(&server.url(), "attention", 3);
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
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_500_returns_err() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(500)
            .create();
        let result = search(&server.url(), "test", 3);
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
        let result = search(&server.url(), "test", 3);
        assert!(result.is_err());
        mock.assert();
    }

    #[test]
    fn get_by_id_returns_paper() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("id_list=2301.08745".to_string()))
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
        let bytes = download_pdf(&server.url(), "2301.08745").unwrap();
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
        let result = download_pdf(&server.url(), "9999.99999");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
