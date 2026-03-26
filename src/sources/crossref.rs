use super::Paper;

/// Parse CrossRef JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let items = root["message"]["items"]
        .as_array()
        .ok_or("missing 'message.items' array")?;

    let mut papers = Vec::new();
    for item in items {
        let doi = item["DOI"].as_str().unwrap_or("").to_string();
        let title = item["title"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let authors: Vec<String> = item["author"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|a| {
                        let given = a["given"].as_str().unwrap_or("");
                        let family = a["family"].as_str().unwrap_or("");
                        format!("{} {}", given, family).trim().to_string()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let year = extract_year(item, "published")
            .or_else(|| extract_year(item, "issued"))
            .or_else(|| extract_year(item, "created"));

        let url = item["URL"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("https://doi.org/{}", doi));

        let venue = item["container-title"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let citations = item["is-referenced-by-count"].as_u64().map(|n| n as u32);

        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        papers.push(Paper {
            id: doi.clone(),
            title,
            authors,
            abstract_text,
            year,
            doi: Some(doi),
            url: Some(url),
            pdf_url: None,
            venue,
            citations,
            fields: vec![],
            open_access: None,
            source: "crossref".to_string(),
        });
    }

    Ok(papers)
}

fn extract_year(item: &serde_json::Value, field: &str) -> Option<u16> {
    item[field]["date-parts"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|parts| parts.as_array())
        .and_then(|parts| parts.first())
        .and_then(|y| y.as_u64())
        .map(|y| y as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/crossref_search.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_titles_from_array() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_given_family() {
        let papers = parse_search_response(FIXTURE).unwrap();
        // At least one paper should have authors
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.is_empty(), "empty author name in paper {}", p.id);
            }
        }
    }

    #[test]
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.doi.is_some(), "paper missing DOI");
            assert!(!p.doi.as_ref().unwrap().is_empty());
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
    fn parse_year_from_date_parts() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_year: Vec<_> = papers.iter().filter(|p| p.year.is_some()).collect();
        assert!(!with_year.is_empty(), "no papers with year");
        for p in &with_year {
            assert!(p.year.unwrap() > 2000);
        }
    }

    #[test]
    fn parse_source_is_crossref() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "crossref");
        }
    }
}
