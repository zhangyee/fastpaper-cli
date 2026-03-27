use quick_xml::events::Event;
use quick_xml::Reader;

use super::Paper;

/// Parse PMC efetch XML response into a list of Papers.
pub fn parse_efetch_response(xml: &str) -> Result<Vec<Paper>, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut papers = Vec::new();

    let mut in_article = false;
    let mut in_front = false;
    let mut in_contrib_author = false;
    let mut tag_stack: Vec<String> = Vec::new();

    let mut pmcid = String::new();
    let mut doi = String::new();
    let mut title = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut surname = String::new();
    let mut given_names = String::new();
    let mut abstract_text = String::new();
    let mut year = String::new();
    let mut journal = String::new();
    let mut in_abstract = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "article" => {
                        in_article = true;
                        pmcid.clear();
                        doi.clear();
                        title.clear();
                        authors.clear();
                        abstract_text.clear();
                        year.clear();
                        journal.clear();
                    }
                    "front" if in_article => in_front = true,
                    "article-id" if in_front => {
                        let mut id_type = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"pub-id-type" {
                                id_type = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        tag_stack.push(format!("article-id:{}", id_type));
                        buf.clear();
                        continue;
                    }
                    "contrib" if in_front => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"contrib-type"
                                && attr.value.as_ref() == b"author"
                            {
                                in_contrib_author = true;
                                surname.clear();
                                given_names.clear();
                            }
                        }
                    }
                    "abstract" if in_front => in_abstract = true,
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
                if current == "article-id:pmcid" && pmcid.is_empty() {
                    pmcid.push_str(text.trim());
                } else if current == "article-id:doi" && doi.is_empty() {
                    doi.push_str(text.trim());
                } else if current == "article-title" && in_front {
                    title.push_str(&text);
                } else if current == "surname" && in_contrib_author {
                    surname.push_str(text.trim());
                } else if current == "given-names" && in_contrib_author {
                    given_names.push_str(text.trim());
                } else if in_abstract && !text.trim().is_empty() {
                    if !abstract_text.is_empty() {
                        abstract_text.push(' ');
                    }
                    abstract_text.push_str(text.trim());
                } else if current == "year" && in_front && year.is_empty() {
                    year.push_str(text.trim());
                } else if current == "journal-title" && in_front && journal.is_empty() {
                    journal.push_str(text.trim());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    "article" => {
                        if !pmcid.is_empty() && !title.is_empty() {
                            papers.push(Paper {
                                id: pmcid.clone(),
                                title: title.trim().to_string(),
                                authors: authors.clone(),
                                abstract_text: if abstract_text.is_empty() {
                                    None
                                } else {
                                    Some(abstract_text.clone())
                                },
                                year: year.parse::<u16>().ok(),
                                doi: if doi.is_empty() { None } else { Some(doi.clone()) },
                                url: Some(format!(
                                    "https://www.ncbi.nlm.nih.gov/pmc/articles/{}/",
                                    pmcid
                                )),
                                pdf_url: Some(format!(
                                    "https://www.ncbi.nlm.nih.gov/pmc/articles/{}/pdf/",
                                    pmcid
                                )),
                                venue: if journal.is_empty() {
                                    None
                                } else {
                                    Some(journal.clone())
                                },
                                citations: None,
                                fields: vec![],
                                open_access: Some(true),
                                source: "pmc".to_string(),
                            });
                        }
                        in_article = false;
                        in_front = false;
                    }
                    "front" => in_front = false,
                    "contrib" if in_contrib_author => {
                        let name_str =
                            format!("{} {}", given_names, surname).trim().to_string();
                        if !name_str.is_empty() {
                            authors.push(name_str);
                        }
                        in_contrib_author = false;
                    }
                    "abstract" => in_abstract = false,
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

    const FIXTURE: &str = include_str!("../../tests/fixtures/pmc_efetch.xml");

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
    fn parse_source_is_pmc() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "pmc");
        }
    }

    #[test]
    fn parse_id_is_pmc_format() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.id.starts_with("PMC"), "id should start with PMC: {}", p.id);
        }
    }

    #[test]
    fn parse_title_not_empty() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.title.is_empty(), "paper {} has empty title", p.id);
        }
    }

    #[test]
    fn parse_authors_surname_given() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_authors: Vec<_> = papers.iter().filter(|p| !p.authors.is_empty()).collect();
        assert!(!with_authors.is_empty(), "no papers with authors");
    }

    #[test]
    fn parse_doi() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_doi: Vec<_> = papers.iter().filter(|p| p.doi.is_some()).collect();
        assert!(!with_doi.is_empty(), "no papers with DOI");
        for p in &with_doi {
            assert!(p.doi.as_ref().unwrap().starts_with("10."));
        }
    }

    #[test]
    fn parse_year() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(p.year.is_some(), "paper {} missing year", p.id);
            assert!(p.year.unwrap() >= 2000);
        }
    }

    #[test]
    fn parse_venue() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_venue: Vec<_> = papers.iter().filter(|p| p.venue.is_some()).collect();
        assert!(!with_venue.is_empty(), "no papers with venue");
    }

    #[test]
    fn parse_abstract() {
        let papers = parse_efetch_response(FIXTURE).unwrap();
        let with_abstract: Vec<_> = papers.iter().filter(|p| p.abstract_text.is_some()).collect();
        assert!(!with_abstract.is_empty(), "no papers with abstract");
        for p in &with_abstract {
            assert!(p.abstract_text.as_ref().unwrap().len() > 10);
        }
    }
}
