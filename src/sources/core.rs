use super::Paper;

/// Parse CORE JSON search response into a list of Papers.
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

        let id = item["id"].as_u64().map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());
        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        let download_url = item["downloadUrl"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let citations = item["citationCount"].as_u64().map(|n| n as u32);

        let year = item["publishedDate"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok())
            .or_else(|| item["yearPublished"].as_u64().map(|y| y as u16));

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["links"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|l| l["url"].as_str())
                .map(|s| s.to_string()),
            pdf_url: download_url,
            venue: None,
            citations,
            fields: item["fieldOfStudy"]
                .as_str()
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
            open_access: Some(true),
            source: "core".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/core_search.json");

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
    fn parse_source_is_core() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "core");
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
    fn parse_abstract() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
    }

    #[test]
    fn parse_pdf_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
    }

    #[test]
    fn parse_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
        }
    }
}
