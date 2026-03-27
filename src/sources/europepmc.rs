use super::Paper;

/// Parse Europe PMC JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["resultList"]["result"]
        .as_array()
        .ok_or("missing 'resultList.result' array")?;

    let mut papers = Vec::new();
    for item in results {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let id = item["id"].as_str().unwrap_or("").to_string();

        let authors: Vec<String> = item["authorList"]["author"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["fullName"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());
        let abstract_text = item["abstractText"].as_str().map(|s| s.to_string());
        let year = item["pubYear"]
            .as_str()
            .and_then(|s| s.parse::<u16>().ok());
        let citations = item["citedByCount"].as_u64().map(|n| n as u32);

        let is_oa = item["isOpenAccess"]
            .as_str()
            .map(|s| s == "Y");

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["fullTextUrlList"]["fullTextUrl"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|u| u["url"].as_str())
                .map(|s| s.to_string()),
            pdf_url: None,
            venue: item["journalTitle"].as_str().map(|s| s.to_string()),
            citations,
            fields: vec![],
            open_access: is_oa,
            source: "europepmc".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/europepmc_search.json");

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
    fn parse_source_is_europepmc() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "europepmc");
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
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
        }
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_oa: Vec<_> = papers.iter().filter(|p| p.open_access.is_some()).collect();
        assert!(!with_oa.is_empty(), "no papers with open_access");
        // open_access should be a bool, not a string
        for p in &with_oa {
            let _ = p.open_access.unwrap(); // just confirm it's bool
        }
    }
}
