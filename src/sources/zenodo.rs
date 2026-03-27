use super::Paper;

/// Parse Zenodo JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let hits = root["hits"]["hits"]
        .as_array()
        .ok_or("missing 'hits.hits' array")?;

    let mut papers = Vec::new();
    for item in hits {
        let meta = &item["metadata"];

        let title = meta["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let authors: Vec<String> = meta["creators"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = item["doi"].as_str().map(|s| s.to_string());

        let abstract_text = meta["description"].as_str().map(|s| s.to_string());

        let year = meta["publication_date"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let pdf_url = item["files"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|f| {
                        f["key"]
                            .as_str()
                            .map(|k| k.ends_with(".pdf"))
                            .unwrap_or(false)
                    })
                    .and_then(|f| f["links"]["self"].as_str().map(|s| s.to_string()))
            });

        let is_open = meta["access_right"].as_str() == Some("open");

        let id = item["id"].as_u64().map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: item["links"]["html"].as_str().map(|s| s.to_string()),
            pdf_url,
            venue: None,
            citations: None,
            fields: vec![],
            open_access: Some(is_open),
            source: "zenodo".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/zenodo_search.json");

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
    fn parse_source_is_zenodo() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "zenodo");
        }
    }

    #[test]
    fn parse_title() {
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
    fn parse_year() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_pdf_url() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
    }

    #[test]
    fn parse_open_access() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.open_access.is_some(), "paper {} missing open_access", p.id);
        }
    }
}
