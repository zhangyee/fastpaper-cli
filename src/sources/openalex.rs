use super::Paper;

const DEFAULT_EMAIL: &str = "yee.zhang@gmail.com";

/// Search OpenAlex API.
pub fn search(base_url: &str, query: &str, max_results: u32) -> Result<Vec<Paper>, String> {
    let mailto = std::env::var("FASTPAPER_EMAIL").unwrap_or_else(|_| DEFAULT_EMAIL.to_string());
    let url = format!(
        "{}/works?search={}&per_page={}&mailto={}",
        base_url, query, max_results, mailto
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

/// Parse OpenAlex JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array")?;

    let mut papers = Vec::new();
    for item in results {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let paper_id = item["id"]
            .as_str()
            .unwrap_or("")
            .replace("https://openalex.org/", "");

        let authors: Vec<String> = item["authorships"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["author"]["display_name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"]
            .as_str()
            .map(|s| s.replace("https://doi.org/", ""));

        let abstract_text = reconstruct_abstract(&item["abstract_inverted_index"]);

        let year = item["publication_year"].as_u64().map(|y| y as u16);
        let citations = item["cited_by_count"].as_u64().map(|n| n as u32);
        let is_oa = item["open_access"]["is_oa"].as_bool();

        let url = item["primary_location"]["landing_page_url"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .map(|s| s.to_string());

        let pdf_url = item["primary_location"]["pdf_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                if is_oa == Some(true) {
                    item["open_access"]["oa_url"].as_str().filter(|s| !s.is_empty())
                } else {
                    None
                }
            })
            .map(|s| s.to_string());

        let venue = item["primary_location"]["source"]["display_name"]
            .as_str()
            .map(|s| s.to_string());

        let fields: Vec<String> = item["concepts"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(5)
                    .filter_map(|c| c["display_name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        papers.push(Paper {
            id: paper_id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url,
            pdf_url,
            venue,
            citations,
            fields,
            open_access: is_oa,
            source: "openalex".to_string(),
        });
    }

    Ok(papers)
}

/// Reconstruct abstract text from OpenAlex inverted index format.
fn reconstruct_abstract(inverted_index: &serde_json::Value) -> Option<String> {
    let obj = inverted_index.as_object()?;
    let mut word_positions: Vec<(u64, &str)> = Vec::new();
    for (word, positions) in obj {
        if let Some(arr) = positions.as_array() {
            for pos in arr {
                if let Some(p) = pos.as_u64() {
                    word_positions.push((p, word));
                }
            }
        }
    }
    if word_positions.is_empty() {
        return None;
    }
    word_positions.sort_by_key(|(pos, _)| *pos);
    Some(word_positions.iter().map(|(_, word)| *word).collect::<Vec<&str>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/openalex_search.json");

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
    fn parse_source_is_openalex() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "openalex");
        }
    }

    #[test]
    fn parse_titles_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_from_authorships() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.authors.is_empty(), "paper {} has no authors", p.id);
        }
    }

    #[test]
    fn parse_doi_strips_prefix() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            if let Some(ref doi) = p.doi {
                assert!(
                    !doi.starts_with("https://"),
                    "DOI should not have URL prefix: {}",
                    doi
                );
            }
        }
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
            assert!(p.citations.unwrap() > 0);
        }
    }

    #[test]
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.open_access.is_some(), "paper {} missing open_access", p.id);
        }
    }

    #[test]
    fn parse_abstract_from_inverted_index() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // Result 1 has abstract_inverted_index
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "at least one paper should have abstract");
        for p in &with_abstract {
            let text = p.abstract_text.as_ref().unwrap();
            assert!(text.len() > 10, "abstract too short: {}", text);
        }
    }

    #[test]
    fn parse_empty_results() {
        let papers = parse_search_response(r#"{"results": []}"#).unwrap();
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
    fn search_request_path_contains_works() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("/works".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_search_param() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("search=test".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_per_page() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("per_page=3".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_request_contains_mailto() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("mailto=".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        mock.assert();
    }

    #[test]
    fn search_uses_env_email_when_set() {
        unsafe { std::env::set_var("FASTPAPER_EMAIL", "custom@example.com") };
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Regex("mailto=custom%40example.com|mailto=custom@example.com".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), "test", 3);
        unsafe { std::env::remove_var("FASTPAPER_EMAIL") };
        mock.assert();
    }
}
