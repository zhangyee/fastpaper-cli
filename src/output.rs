use crate::sources::Paper;

/// Format papers as CSV.
pub fn to_csv(papers: &[Paper]) -> String {
    let mut out = String::from("id,title,authors,year,doi,url\n");
    for p in papers {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_escape(&p.id),
            csv_escape(&p.title),
            csv_escape(&p.authors.join(";")),
            p.year.map(|y| y.to_string()).unwrap_or_default(),
            csv_escape(p.doi.as_deref().unwrap_or("")),
            csv_escape(p.url.as_deref().unwrap_or("")),
        ));
    }
    out
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Format papers as BibTeX entries.
pub fn to_bibtex(papers: &[Paper]) -> String {
    let mut out = String::new();
    for p in papers {
        out.push_str(&format!("@article{{{},\n", p.id));
        out.push_str(&format!("  title = {{{}}},\n", p.title));
        out.push_str(&format!("  author = {{{}}},\n", p.authors.join(" and ")));
        if let Some(year) = p.year {
            out.push_str(&format!("  year = {{{}}},\n", year));
        }
        if let Some(ref doi) = p.doi {
            out.push_str(&format!("  doi = {{{}}},\n", doi));
        }
        out.push_str("}\n");
    }
    out
}

/// Format papers as a JSON search result string.
pub fn to_json(papers: &[Paper]) -> String {
    let result = serde_json::json!({
        "source": papers.first().map(|p| p.source.as_str()).unwrap_or(""),
        "results": papers,
    });
    serde_json::to_string_pretty(&result).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_paper() -> Paper {
        Paper {
            id: "2301.08745".to_string(),
            title: "Attention Is All You Need".to_string(),
            authors: vec!["Alice Smith".to_string(), "Bob Jones".to_string()],
            abstract_text: Some("We propose a new architecture.".to_string()),
            year: Some(2023),
            doi: Some("10.48550/arXiv.2301.08745".to_string()),
            url: Some("https://arxiv.org/abs/2301.08745".to_string()),
            pdf_url: Some("https://arxiv.org/pdf/2301.08745".to_string()),
            venue: Some("arXiv preprint".to_string()),
            citations: None,
            fields: vec!["cs.CL".to_string()],
            open_access: Some(true),
            source: "arxiv".to_string(),
        }
    }

    #[test]
    fn to_json_returns_valid_json() {
        let json = to_json(&[sample_paper()]);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should be valid JSON");
        assert!(parsed.is_object());
    }

    #[test]
    fn to_json_has_source_field() {
        let json = to_json(&[sample_paper()]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["source"], "arxiv");
    }

    #[test]
    fn to_json_has_results_array() {
        let json = to_json(&[sample_paper()]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = v["results"].as_array().expect("results should be array");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn to_json_paper_has_expected_fields() {
        let json = to_json(&[sample_paper()]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let paper = &v["results"][0];
        assert_eq!(paper["id"], "2301.08745");
        assert_eq!(paper["title"], "Attention Is All You Need");
        assert_eq!(paper["authors"][0], "Alice Smith");
        assert_eq!(paper["year"], 2023);
        assert_eq!(paper["doi"], "10.48550/arXiv.2301.08745");
        assert_eq!(paper["url"], "https://arxiv.org/abs/2301.08745");
    }

    #[test]
    fn to_json_empty_papers() {
        let json = to_json(&[]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = v["results"].as_array().expect("results should be array");
        assert!(results.is_empty());
    }

    #[test]
    fn to_csv_first_line_is_header() {
        let csv = to_csv(&[sample_paper()]);
        let first_line = csv.lines().next().unwrap();
        assert_eq!(first_line, "id,title,authors,year,doi,url");
    }

    #[test]
    fn to_csv_second_line_contains_id() {
        let csv = to_csv(&[sample_paper()]);
        let second_line = csv.lines().nth(1).unwrap();
        assert!(second_line.starts_with("2301.08745,"));
    }

    #[test]
    fn to_csv_authors_joined_by_semicolon() {
        let csv = to_csv(&[sample_paper()]);
        let second_line = csv.lines().nth(1).unwrap();
        assert!(second_line.contains("Alice Smith;Bob Jones"));
    }

    #[test]
    fn to_csv_quotes_field_with_comma() {
        let mut paper = sample_paper();
        paper.title = "Attention, Transformers, and You".to_string();
        let csv = to_csv(&[paper]);
        let second_line = csv.lines().nth(1).unwrap();
        assert!(second_line.contains("\"Attention, Transformers, and You\""));
    }

    #[test]
    fn to_csv_empty_returns_header_only() {
        let csv = to_csv(&[]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "id,title,authors,year,doi,url");
    }

    #[test]
    fn to_bibtex_contains_article_tag() {
        let bib = to_bibtex(&[sample_paper()]);
        assert!(bib.contains("@article{"));
    }

    #[test]
    fn to_bibtex_contains_title() {
        let bib = to_bibtex(&[sample_paper()]);
        assert!(bib.contains("title = {Attention Is All You Need}"));
    }

    #[test]
    fn to_bibtex_contains_author() {
        let bib = to_bibtex(&[sample_paper()]);
        assert!(bib.contains("author = {Alice Smith and Bob Jones}"));
    }

    #[test]
    fn to_bibtex_contains_year() {
        let bib = to_bibtex(&[sample_paper()]);
        assert!(bib.contains("year = {2023}"));
    }

    #[test]
    fn to_bibtex_contains_doi() {
        let bib = to_bibtex(&[sample_paper()]);
        assert!(bib.contains("doi = {10.48550/arXiv.2301.08745}"));
    }

    #[test]
    fn to_bibtex_no_doi_when_missing() {
        let mut paper = sample_paper();
        paper.doi = None;
        let bib = to_bibtex(&[paper]);
        assert!(!bib.contains("doi ="));
    }

    #[test]
    fn to_bibtex_empty_returns_empty_string() {
        let bib = to_bibtex(&[]);
        assert!(bib.is_empty());
    }
}
