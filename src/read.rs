use std::path::Path;

/// Extract full text from a local PDF file.
pub fn extract_text(path: &Path) -> Result<String, String> {
    let mut doc = pdf_oxide::PdfDocument::open(path)
        .map_err(|e| format!("Failed to open PDF: {}", e))?;
    let text = doc
        .extract_all_text()
        .map_err(|e| format!("Failed to extract text: {}", e))?;
    Ok(text)
}

/// Extract only the abstract section from PDF text.
pub fn extract_section_abstract(full_text: &str) -> Option<String> {
    let lower = full_text.to_lowercase();
    let start = lower.find("abstract")?;
    let after_abstract = start + "abstract".len();
    // Find next section heading
    let section_headings = [
        "introduction",
        "background",
        "related work",
        "methods",
        "methodology",
        "results",
        "discussion",
        "conclusion",
    ];
    let end = section_headings
        .iter()
        .filter_map(|h| lower[after_abstract..].find(h).map(|i| after_abstract + i))
        .min()
        .unwrap_or(full_text.len());
    let section = full_text[after_abstract..end].trim();
    if section.is_empty() {
        None
    } else {
        Some(section.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.pdf")
    }

    // Behavior 1: read local test.pdf → output contains text
    #[test]
    fn extract_text_returns_content() {
        let text = extract_text(&fixture_path()).unwrap();
        assert!(!text.is_empty(), "extracted text should not be empty");
    }

    #[test]
    fn extract_text_contains_expected_words() {
        let text = extract_text(&fixture_path()).unwrap();
        assert!(
            text.contains("Transformer") || text.contains("attention") || text.contains("Attention"),
            "text should contain expected words, got: {}",
            &text[..text.len().min(200)]
        );
    }

    // Behavior 2: --section abstract → only abstract part
    #[test]
    fn extract_section_abstract_returns_content() {
        let text = extract_text(&fixture_path()).unwrap();
        let abstract_text = extract_section_abstract(&text);
        assert!(abstract_text.is_some(), "should find abstract section");
        let abs = abstract_text.unwrap();
        assert!(!abs.is_empty());
    }

    #[test]
    fn extract_section_abstract_does_not_contain_introduction() {
        let text = extract_text(&fixture_path()).unwrap();
        let abs = extract_section_abstract(&text).unwrap();
        let lower = abs.to_lowercase();
        assert!(
            !lower.contains("introduction"),
            "abstract should not contain introduction heading, got: {}",
            abs
        );
    }

    #[test]
    fn extract_text_nonexistent_file_returns_err() {
        let result = extract_text(Path::new("/nonexistent/fake.pdf"));
        assert!(result.is_err());
    }
}
