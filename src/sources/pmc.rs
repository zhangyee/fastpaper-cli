use quick_xml::events::Event;
use quick_xml::Reader;

use super::Paper;

const ESEARCH_URL: &str = "/entrez/eutils/esearch.fcgi";
const EFETCH_URL: &str = "/entrez/eutils/efetch.fcgi";

/// Search PMC: esearch for IDs, then efetch for details.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let api_key = std::env::var("NCBI_API_KEY").ok();

    // Step 1: esearch
    let mut esearch_url = format!(
        "{}{}?db=pmc&term={}&retmax={}&retmode=json&tool=fastpaper&email=yee.zhang@gmail.com",
        base_url, ESEARCH_URL, query, max_results
    );
    if let Some(ref key) = api_key {
        esearch_url.push_str(&format!("&api_key={}", key));
    }

    let esearch_body = http_get(&esearch_url)?;
    let esearch_json: serde_json::Value =
        serde_json::from_str(&esearch_body).map_err(|e| format!("JSON parse error: {}", e))?;

    let ids: Vec<&str> = esearch_json["esearchresult"]["idlist"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    if ids.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: efetch
    let mut efetch_url = format!(
        "{}{}?db=pmc&id={}&rettype=xml&tool=fastpaper&email=yee.zhang@gmail.com",
        base_url,
        EFETCH_URL,
        ids.join(",")
    );
    if let Some(ref key) = api_key {
        efetch_url.push_str(&format!("&api_key={}", key));
    }

    let efetch_body = http_get(&efetch_url)?;
    parse_efetch_response(&efetch_body)
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

/// Fetch a single paper by PMC ID.
pub fn get_by_pmc_id(base_url: &str, pmc_id: &str) -> Result<Option<Paper>, String> {
    // Strip "PMC" prefix if present to get numeric ID
    let numeric_id = pmc_id.strip_prefix("PMC").unwrap_or(pmc_id);
    let url = format!(
        "{}/entrez/eutils/efetch.fcgi?db=pmc&id={}&rettype=xml&tool=fastpaper&email=yee.zhang@gmail.com",
        base_url, numeric_id
    );
    let body = http_get(&url)?;
    let papers = parse_efetch_response(&body)?;
    Ok(papers.into_iter().next())
}

/// Parse PMC efetch XML response into a list of Papers.
pub fn parse_efetch_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut papers = Vec::new();

    let mut in_article = false;
    let mut in_front = false;
    let mut in_contrib_author = false;
    let mut tag_stack: Vec<String> = Vec::new();

    let mut pmcid = String::new();
    let mut doi = String::new();
    let mut title = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut surname = String::new();
    let mut given_names = String::new();
    let mut abstract_text = String::new();
    let mut year = String::new();
    let mut journal = String::new();
    let mut in_abstract = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "article" => {
                        in_article = true;
                        pmcid.clear();
                        doi.clear();
                        title.clear();
                        authors.clear();
                        abstract_text.clear();
                        year.clear();
                        journal.clear();
                    }
                    "front" if in_article => in_front = true,
                    "article-id" if in_front => {
                        let mut id_type = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"pub-id-type" {
                                id_type = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        tag_stack.push(format!("article-id:{}", id_type));
                        buf.clear();
                        continue;
                    }
                    "contrib" if in_front => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"contrib-type"
                                && attr.value.as_ref() == b"author"
                            {
                                in_contrib_author = true;
                                surname.clear();
                                given_names.clear();
                            }
                        }
                    }
                    "abstract" if in_front => in_abstract = true,
                    _ => {}
                }
                tag_stack.push(local.to_string());
            }
            Ok(Event::Text(ref e)) => {
                if !in_article {
                    buf.clear();
                    continue;
                }
                let text = e.decode().unwrap_or_default().to_string();
                let current = tag_stack.last().map(|s| s.as_str()).unwrap_or("");
                if current == "article-id:pmcid" && pmcid.is_empty() {
                    pmcid.push_str(text.trim());
                } else if current == "article-id:doi" && doi.is_empty() {
                    doi.push_str(text.trim());
                } else if current == "article-title" && in_front {
                    title.push_str(&text);
                } else if current == "surname" && in_contrib_author {
                    surname.push_str(text.trim());
                } else if current == "given-names" && in_contrib_author {
                    given_names.push_str(text.trim());
                } else if in_abstract && !text.trim().is_empty() {
                    if !abstract_text.is_empty() {
                        abstract_text.push(' ');
                    }
                    abstract_text.push_str(text.trim());
                } else if current == "year" && in_front && year.is_empty() {
                    year.push_str(text.trim());
                } else if current == "journal-title" && in_front && journal.is_empty() {
                    journal.push_str(text.trim());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "article" => {
                        if !pmcid.is_empty() && !title.is_empty() {
                            papers.push(Paper {
                                id: pmcid.clone(),
                                title: title.trim().to_string(),
                                authors: authors.clone(),
                                abstract_text: if abstract_text.is_empty() {
                                    None
                                } else {
                                    Some(abstract_text.clone())
                                },
                                year: year.parse::<u16>().ok(),
                                doi: if doi.is_empty() { None } else { Some(doi.clone()) },
                                url: Some(format!(
                                    "https://www.ncbi.nlm.nih.gov/pmc/articles/{}/",
                                    pmcid
                                )),
                                pdf_url: Some(format!(
                                    "https://www.ncbi.nlm.nih.gov/pmc/articles/{}/pdf/",
                                    pmcid
                                )),
                                venue: if journal.is_empty() {
                                    None
                                } else {
                                    Some(journal.clone())
                                },
                                citations: None,
                                fields: vec![],
                                open_access: Some(true),
                                source: "pmc".to_string(),
                            });
                        }
                        in_article = false;
                        in_front = false;
                    }
                    "front" => in_front = false,
                    "contrib" if in_contrib_author => {
                        let name_str =
                            format!("{} {}", given_names, surname).trim().to_string();
                        if !name_str.is_empty() {
                            authors.push(name_str);
                        }
                        in_contrib_author = false;
                    }
                    "abstract" => in_abstract = false,
                    _ => {}
                }
                tag_stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pmc_efetch.xml");

    #[test]
    fn parse_returns_ok() {
        let result = parse_efetch_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_papers_not_empty() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        assert!(!papers.is_empty());
    }

    #[test]
    fn parse_source_is_pmc() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "pmc");
        }
    }

    #[test]
    fn parse_id_is_pmc_format() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.id.starts_with("PMC"), "id should start with PMC: {}", p.id);
        }
    }

    #[test]
    fn parse_title_not_empty() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_surname_given() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_year() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.year.is_some(), "paper {} missing year", p.id);
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_venue() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_venue: Vec<_> = papers.iter().filter(|p| p.venue.is_some()).collect();
        assert!(!with_venue.is_empty(), "no papers with venue");
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
        for p in &with_abstract {
            assert!(p.abstract_text.as_ref().unwrap().len() > 10);
        }
    }

    const ESEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/pmc_esearch.json");

    #[test]
    fn search_calls_esearch_then_efetch() {
        let mut server = mockito::Server::new();
        let esearch_mock = server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        let efetch_mock = server
            .mock("GET", mockito::Matcher::Regex("efetch".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(&server.url(), "test", 3).unwrap();
        assert!(!papers.is_empty());
        esearch_mock.assert();
        efetch_mock.assert();
    }

    #[test]
    fn search_esearch_contains_db_pmc() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("esearch.*db=pmc".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_efetch_contains_rettype_xml() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("efetch.*rettype=xml".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    #[serial]
    fn search_with_api_key() {
        unsafe { std::env::set_var("NCBI_API_KEY", "pmc-test-key") };
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch.*api_key=pmc-test-key".to_string()))
            .with_status(200)
            .with_body(ESEARCH_FIXTURE)
            .create();
        server
            .mock("GET", mockito::Matcher::Regex("efetch.*api_key=pmc-test-key".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let result = search(&server.url(), "test", 3);
        unsafe { std::env::remove_var("NCBI_API_KEY") };
        assert!(result.is_ok());
    }

    #[test]
    fn search_empty_ids_skips_efetch() {
        let empty_esearch = r#"{"esearchresult":{"idlist":[]}}"#;
        let mut server = mockito::Server::new();
        server
            .mock("GET", mockito::Matcher::Regex("esearch".to_string()))
            .with_status(200)
            .with_body(empty_esearch)
            .create();
        // efetch should NOT be called
        let efetch_mock = server
            .mock("GET", mockito::Matcher::Regex("efetch".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .expect(0)
            .create();
        let papers = search(&server.url(), "nonexistent", 3).unwrap();
        assert!(papers.is_empty());
        efetch_mock.assert();
    }
}
