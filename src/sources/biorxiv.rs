use super::Paper;

/// Parse BioRxiv JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let collection = root["collection"]
        .as_array()
        .ok_or("missing 'collection' array")?;

    let mut papers = Vec::new();
    for item in collection {
        let title = item["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }

        let doi = item["doi"].as_str().unwrap_or("").to_string();
        let version = item["version"].as_str().unwrap_or("1");

        let authors: Vec<String> = item["authors"]
            .as_str()
            .map(|s| s.split(';').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect())
            .unwrap_or_default();

        let abstract_text = item["abstract"].as_str().map(|s| s.to_string());

        let year = item["date"]
            .as_str()
            .and_then(|s| s.get(..4))
            .and_then(|y| y.parse::<u16>().ok());

        let category = item["category"]
            .as_str()
            .map(|s| s.to_string());

        let pdf_url = if !doi.is_empty() {
            Some(format!("https://www.biorxiv.org/content/{}v{}.full.pdf", doi, version))
        } else {
            None
        };

        let url = if !doi.is_empty() {
            Some(format!("https://www.biorxiv.org/content/{}v{}", doi, version))
        } else {
            None
        };

        papers.push(Paper {
            id: doi.clone(),
            title,
            authors,
            abstract_text,
            year,
            doi: if doi.is_empty() { None } else { Some(doi) },
            url,
            pdf_url,
            venue: Some("bioRxiv preprint".to_string()),
            citations: None,
            fields: category.into_iter().collect(),
            open_access: Some(true),
            source: "biorxiv".to_string(),
        });
    }

    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/biorxiv_search.json");

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
    fn parse_source_is_biorxiv() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "biorxiv");
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
    fn parse_doi() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_authors_split_by_semicolon() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.contains(';'), "author should not contain semicolon: {}", a);
            }
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
    fn parse_category_to_fields() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_fields: Vec<_> = papers.iter().filter(|p| !p.fields.is_empty()).collect();
        assert!(!with_fields.is_empty(), "no papers with fields");
    }

    #[test]
    fn parse_pdf_url_from_doi_and_version() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let with_pdf: Vec<_> = papers.iter().filter(|p| p.pdf_url.is_some()).collect();
        assert!(!with_pdf.is_empty(), "no papers with pdf_url");
        for p in &with_pdf {
            let url = p.pdf_url.as_ref().unwrap();
            assert!(url.contains("biorxiv.org"), "pdf_url should contain biorxiv.org: {}", url);
            assert!(url.ends_with(".full.pdf"), "pdf_url should end with .full.pdf: {}", url);
        }
    }
}
