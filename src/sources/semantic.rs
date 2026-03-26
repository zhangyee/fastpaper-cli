use super::Paper;

/// Parse Semantic Scholar JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let data = root["data"]
        .as_array()
        .ok_or("missing 'data' array")?;

    let mut papers = Vec::new();
    for item in data {
        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["externalIds"]["DOI"].as_str().map(|s| s.to_string());

        let pdf_url = item["openAccessPdf"]["url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.contains("arxiv.org/abs/") {
                    s.replace("/abs/", "/pdf/")
                } else {
                    s.to_string()
                }
            });

        let fields: Vec<String> = item["fieldsOfStudy"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let citations = item["citationCount"].as_u64().map(|n| n as u32);

        papers.push(Paper {
            id: item["paperId"].as_str().unwrap_or("").to_string(),
            title: item["title"].as_str().unwrap_or("").to_string(),
            authors,
            abstract_text: item["abstract"].as_str().map(|s| s.to_string()),
            year: item["year"].as_u64().map(|y| y as u16),
            doi,
            url: item["url"].as_str().map(|s| s.to_string()),
            pdf_url,
            venue: item["venue"].as_str().map(|s| s.to_string()),
            citations,
            fields,
            open_access: Some(item["openAccessPdf"].is_object() && !item["openAccessPdf"].is_null()),
            source: "semantic".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/semantic_search.json");

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
    fn parse_source_is_semantic() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "semantic");
        }
    }

    #[test]
    fn parse_citations_present() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.citations.is_some(), "paper {} missing citations", p.id);
            assert!(p.citations.unwrap() > 0);
        }
    }

    #[test]
    fn parse_pdf_url_from_open_access() {
        // Our fixture has openAccessPdf as null for all papers
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.pdf_url.is_none(), "paper {} should have no pdf_url", p.id);
        }
    }

    #[test]
    fn parse_doi_from_external_ids() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // First paper has DOI in externalIds
        let first = &papers[0];
        assert_eq!(
            first.doi.as_deref(),
            Some("10.1016/J.NEUCOM.2021.03.091")
        );
    }

    #[test]
    fn parse_empty_data_returns_empty_list() {
        let papers = parse_search_response(r#"{"data": []}"#).unwrap();
        assert!(papers.is_empty());
    }
}
