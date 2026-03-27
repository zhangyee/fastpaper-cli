use super::Paper;

/// Parse DOAJ JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let results = root["results"]
        .as_array()
        .ok_or("missing 'results' array")?;

    let mut papers = Vec::new();
    for item in results {
        let bib = &item["bibjson"];

        let title = bib["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let authors: Vec<String> = bib["author"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let doi = bib["identifier"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|id| id["type"].as_str() == Some("doi"))
                    .and_then(|id| id["id"].as_str().map(|s| s.to_string()))
            });

        let abstract_text = bib["abstract"].as_str().map(|s| s.to_string());

        let year = bib["year"]
            .as_str()
            .and_then(|s| s.parse::<u16>().ok());

        let pdf_url = bib["link"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|l| {
                        l["type"].as_str().map(|t| t.contains("fulltext")).unwrap_or(false)
                    })
                    .and_then(|l| l["url"].as_str().map(|s| s.to_string()))
            });

        let id = item["id"].as_str().unwrap_or("").to_string();

        papers.push(Paper {
            id,
            title,
            authors,
            abstract_text,
            year,
            doi,
            url: pdf_url.clone(),
            pdf_url,
            venue: bib["journal"]["title"].as_str().map(|s| s.to_string()),
            citations: None,
            fields: vec![],
            open_access: Some(true),
            source: "doaj".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/doaj_search.json");

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
    fn parse_source_is_doaj() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "doaj");
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
    fn parse_open_access_always_true() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.open_access, Some(true), "DOAJ papers are always OA");
        }
    }

    #[test]
    fn parse_pdf_url_from_fulltext_link() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
        for p in &with_pdf {
            assert!(p.pdf_url.as_ref().unwrap().starts_with("http"));
        }
    }
}
