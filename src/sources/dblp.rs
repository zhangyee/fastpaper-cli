use quick_xml::events::Event;
use quick_xml::Reader;

use super::Paper;

/// Search DBLP API.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let url = format!(
        "{}/search/publ/api?q={}&format=xml&h={}",
        base_url, query, max_results
    );

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
        match ureq::get(&url).call() {
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

/// Parse DBLP XML search response into a list of Papers.
pub fn parse_search_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut papers = Vec::new();

    let mut in_hit = false;
    let mut in_info = false;
    let mut in_authors = false;
    let mut tag = String::new();

    let mut title = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut year = String::new();
    let mut venue = String::new();
    let mut doi = String::new();
    let mut ee = String::new();
    let mut dblp_url = String::new();
    let mut dblp_key = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = std::str::from_utf8(name.as_ref()).unwrap_or("");
                match local {
                    "hit" => {
                        in_hit = true;
                        title.clear();
                        authors.clear();
                        year.clear();
                        venue.clear();
                        doi.clear();
                        ee.clear();
                        dblp_url.clear();
                        dblp_key.clear();
                    }
                    "info" if in_hit => in_info = true,
                    "authors" if in_info => in_authors = true,
                    _ if in_info => {
                        tag = local.to_string();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if !in_info {
                    buf.clear();
                    continue;
                }
                let text = e.decode().unwrap_or_default().to_string();
                match tag.as_str() {
                    "title" => title.push_str(&text),
                    "author" if in_authors => {
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            authors.push(trimmed);
                        }
                    }
                    "year" => year.push_str(text.trim()),
                    "venue" => venue.push_str(&text),
                    "doi" => doi.push_str(text.trim()),
                    "ee" => ee.push_str(text.trim()),
                    "url" => dblp_url.push_str(text.trim()),
                    "key" => dblp_key.push_str(text.trim()),
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = std::str::from_utf8(name.as_ref()).unwrap_or("");
                match local {
                    "hit" => {
                        if !title.is_empty() {
                            let extracted_doi = if !doi.is_empty() {
                                Some(doi.clone())
                            } else if ee.contains("doi.org/") {
                                ee.split("doi.org/").nth(1).map(|s| s.to_string())
                            } else {
                                None
                            };

                            papers.push(Paper {
                                id: dblp_key.clone(),
                                title: title.trim().to_string(),
                                authors: authors.clone(),
                                abstract_text: None,
                                year: year.parse::<u16>().ok(),
                                doi: extracted_doi,
                                url: if dblp_url.is_empty() {
                                    None
                                } else {
                                    Some(dblp_url.clone())
                                },
                                pdf_url: None,
                                venue: if venue.is_empty() {
                                    None
                                } else {
                                    Some(venue.trim().to_string())
                                },
                                citations: None,
                                fields: vec![],
                                open_access: None,
                                source: "dblp".to_string(),
                            });
                        }
                        in_hit = false;
                        in_info = false;
                    }
                    "info" => in_info = false,
                    "authors" => in_authors = false,
                    _ => {}
                }
                tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/dblp_search.xml");

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
    fn parse_source_is_dblp() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "dblp");
        }
    }

    #[test]
    fn parse_title_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.authors.is_empty(), "paper {} has no authors", p.id);
        }
    }

    #[test]
    fn parse_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.year.is_some(), "paper {} missing year", p.id);
        }
    }

    #[test]
    fn parse_venue() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_venue: Vec<_> = papers.iter().filter(|p| p.venue.is_some()).collect();
        assert!(!with_venue.is_empty(), "no papers with venue");
    }

    #[test]
    fn parse_doi_from_ee() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            let doi = p.doi.as_ref().unwrap();
            assert!(doi.starts_with("10."), "DOI should start with 10.: {}", doi);
        }
    }

    #[test]
    fn parse_abstract_is_none() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.abstract_text.is_none(), "DBLP should not have abstract");
        }
    }

    #[test]
    fn parse_empty_xml() {
        let papers = parse_search_response("<result><hits total=\"0\"></hits></result>").unwrap();
        assert!(papers.is_empty());
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
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/search/publ/api".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_format_xml() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("format=xml".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_h_param() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("h=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }
}
