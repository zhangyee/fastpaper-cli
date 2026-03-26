use quick_xml::events::Event;
use quick_xml::Reader;

use super::Paper;

/// Parse PubMed efetch XML response into a list of Papers.
pub fn parse_efetch_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut papers = Vec::new();

    // State
    let mut in_article = false;
    let mut in_author = false;
    let mut tag_stack: Vec<String> = Vec::new();
    let mut pmid = String::new();
    let mut title = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut last_name = String::new();
    let mut initials = String::new();
    let mut abstract_text = String::new();
    let mut year = String::new();
    let mut doi = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "PubmedArticle" => {
                        in_article = true;
                        pmid.clear();
                        title.clear();
                        authors.clear();
                        abstract_text.clear();
                        year.clear();
                        doi.clear();
                    }
                    "Author" if in_article => {
                        in_author = true;
                        last_name.clear();
                        initials.clear();
                    }
                    "ELocationID" if in_article => {
                        // Check EIdType attribute
                        let mut is_doi = false;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"EIdType"
                                && attr.value.as_ref() == b"doi"
                            {
                                is_doi = true;
                            }
                        }
                        if is_doi {
                            tag_stack.push("doi".to_string());
                        } else {
                            tag_stack.push(local.to_string());
                        }
                        buf.clear();
                        continue;
                    }
                    _ => {}
                }
                tag_stack.push(local.to_string());
            }
            Ok(Event::Text(ref e)) => {
                if !in_article {
                    buf.clear();
                    continue;
                }
                let text = e.decode().unwrap_or_default().to_string();
                let current = tag_stack.last().map(|s| s.as_str()).unwrap_or("");
                match current {
                    "PMID" if pmid.is_empty() => pmid.push_str(text.trim()),
                    "ArticleTitle" => title.push_str(&text),
                    "LastName" if in_author => last_name.push_str(text.trim()),
                    "Initials" if in_author => initials.push_str(text.trim()),
                    "AbstractText" => {
                        if !abstract_text.is_empty() {
                            abstract_text.push(' ');
                        }
                        abstract_text.push_str(text.trim());
                    }
                    "Year" if year.is_empty() => year.push_str(text.trim()),
                    "doi" => doi.push_str(text.trim()),
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "PubmedArticle" => {
                        if !pmid.is_empty() && !title.is_empty() {
                            papers.push(Paper {
                                id: pmid.clone(),
                                title: title.trim().to_string(),
                                authors: authors.clone(),
                                abstract_text: if abstract_text.is_empty() {
                                    None
                                } else {
                                    Some(abstract_text.clone())
                                },
                                year: year.parse::<u16>().ok(),
                                doi: if doi.is_empty() { None } else { Some(doi.clone()) },
                                url: Some(format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid)),
                                pdf_url: None,
                                venue: None,
                                citations: None,
                                fields: vec![],
                                open_access: None,
                                source: "pubmed".to_string(),
                            });
                        }
                        in_article = false;
                    }
                    "Author" if in_author => {
                        let name_str =
                            format!("{} {}", last_name, initials).trim().to_string();
                        if !name_str.is_empty() {
                            authors.push(name_str);
                        }
                        in_author = false;
                    }
                    _ => {}
                }
                tag_stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/pubmed_efetch.xml");

    #[test]
    fn parse_returns_ok() {
        let result = parse_efetch_response(FIXTURE);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_papers_not_empty() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        assert!(!papers.is_empty());
    }

    #[test]
    fn parse_source_is_pubmed() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "pubmed");
        }
    }

    #[test]
    fn parse_title_from_article_title() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_lastname_initials() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
        for p in &with_authors {
            for a in &p.authors {
                assert!(!a.is_empty());
                // Should have format "LastName Initials"
                assert!(a.len() > 1, "author name too short: {}", a);
            }
        }
    }

    #[test]
    fn parse_abstract_from_abstract_text() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
        for p in &with_abstract {
            assert!(p.abstract_text.as_ref().unwrap().len() > 10);
        }
    }

    #[test]
    fn parse_year_from_pubdate() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.year.is_some(), "paper {} missing year", p.id);
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_doi_from_elocationid() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            let doi = p.doi.as_ref().unwrap();
            assert!(doi.starts_with("10."), "DOI should start with 10.: {}", doi);
        }
    }

    #[test]
    fn parse_id_is_pmid() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(
                p.id.chars().all(|c| c.is_ascii_digit()),
                "PMID should be numeric: {}",
                p.id
            );
        }
    }

    #[test]
    fn parse_pdf_url_is_none() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.pdf_url.is_none(), "pubmed should not have pdf_url");
        }
    }
}
