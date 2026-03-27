use super::Paper;

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
}
