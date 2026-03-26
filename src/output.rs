use crate::sources::Paper;

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
}
