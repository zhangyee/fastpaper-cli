use super::Paper;

/// Search OpenAIRE API.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let encoded = super::encode_query(query);
    let url = format!(
        "{}/search/researchProducts?keywords={}&size={}&format=json",
        base_url, encoded, max_results
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

/// Parse OpenAIRE JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["response"]["results"]["result"]
        .as_array()
        .ok_or("missing 'response.results.result' array")?;

    let mut papers = Vec::new();
    for r in results {
        let meta = &r["metadata"]["oaf:entity"]["oaf:result"];

        // Title: array of objects, find main title
        let title = extract_title(meta);
        if title.is_empty() {
            continue;
        }

        // Creators
        let authors = extract_array_values(meta, "creator");

        // PID → DOI
        let doi = extract_doi_from_pid(meta);

        // Description
        let abstract_text = extract_description(meta);

        // Date
        let year = meta["dateofacceptance"]["$"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let id = r["header"]["dri:objIdentifier"]["$"]
            .as_str()
            .unwrap_or("")
            .to_string();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: None,
            pdf_url: None,
            venue: None,
            citations: None,
            fields: vec![],
            open_access: None,
            source: "openaire".to_string(),
        });
    }

    Ok(papers)
}

fn extract_title(meta: &serde_json::Value) -> String {
    let titles = &meta["title"];
    if let Some(arr) = titles.as_array() {
        for t in arr {
            if t["@classid"].as_str() == Some("main title") {
                if let Some(s) = t["$"].as_str() {
                    return s.to_string();
                }
            }
        }
        // fallback to first
        if let Some(first) = arr.first() {
            if let Some(s) = first["$"].as_str() {
                return s.to_string();
            }
        }
    } else if let Some(s) = titles["$"].as_str() {
        return s.to_string();
    }
    String::new()
}

fn extract_array_values(meta: &serde_json::Value, key: &str) -> Vec<String> {
    let val = &meta[key];
    if let Some(arr) = val.as_array() {
        arr.iter()
            .filter_map(|c| c["$"].as_str().map(|s| s.to_string()))
            .collect()
    } else if let Some(s) = val["$"].as_str() {
        vec![s.to_string()]
    } else {
        vec![]
    }
}

fn extract_doi_from_pid(meta: &serde_json::Value) -> Option<String> {
    let pid = &meta["pid"];
    if let Some(arr) = pid.as_array() {
        for p in arr {
            if p["@classid"].as_str() == Some("doi") {
                return p["$"].as_str().map(|s| s.to_string());
            }
        }
    } else if pid["@classid"].as_str() == Some("doi") {
        return pid["$"].as_str().map(|s| s.to_string());
    }
    None
}

fn extract_description(meta: &serde_json::Value) -> Option<String> {
    let desc = &meta["description"];
    if let Some(arr) = desc.as_array() {
        for d in arr {
            if let Some(s) = d["$"].as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    } else if let Some(s) = desc["$"].as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openaire_search.json");

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
    fn parse_source_is_openaire() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "openaire");
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
    fn parse_authors_from_creator() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi_from_pid() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_abstract_from_description() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_year_from_dateofacceptance() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() >= 2000);
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
    fn search_request_path() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/search/researchProducts".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_keywords() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("keywords=test".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_size() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("size=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_format_json() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("format=json".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }
}
