use std::path::Path;

/// Extract full text from a local PDF file.
pub fn extract_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    extract_text_from_bytes(&bytes)
}

/// Extract full text from PDF bytes in memory.
pub fn extract_text_from_bytes(bytes: &[u8]) -> Result<String, String> {
    let doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| format!("Failed to open PDF: {}", e))?;
    let text = doc
        .extract_all_text()
        .map_err(|e| format!("Failed to extract text: {}", e))?;
    Ok(text)
}

/// Section headings recognised when slicing a paper, in the order they
/// normally appear. Extraction is heuristic: PDF text has no structure, so the
/// best we can do is cut from one known heading to whichever known heading
/// comes next.
const SECTION_HEADINGS: &[&str] = &[
    "abstract",
    "introduction",
    "background",
    "related work",
    "methods",
    "methodology",
    "materials and methods",
    "results",
    "discussion",
    "conclusion",
    "references",
];

/// Extract one section from PDF text by heading name (case-insensitive).
///
/// Returns `None` when the heading is not found or the slice is empty.
pub fn extract_section(full_text: &str, heading: &str) -> Option<String> {
    let lower = full_text.to_lowercase();
    let heading = heading.to_lowercase();
    let start = lower.find(&heading)?;
    let body_start = start + heading.len();

    let end = SECTION_HEADINGS
        .iter()
        .filter(|h| **h != heading)
        .filter_map(|h| lower[body_start..].find(h).map(|i| body_start + i))
        .min()
        .unwrap_or(full_text.len());

    let section = full_text[body_start..end].trim();
    if section.is_empty() {
        None
    } else {
        Some(section.to_string())
    }
}

/// Extract only the abstract section from PDF text.
pub fn extract_section_abstract(full_text: &str) -> Option<String> {
    extract_section(full_text, "abstract")
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
            text.contains("Transformer")
                || text.contains("attention")
                || text.contains("Attention"),
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
    fn extract_text_from_bytes_works() {
        let bytes = std::fs::read(fixture_path()).unwrap();
        let text = extract_text_from_bytes(&bytes).unwrap();
        assert!(!text.is_empty());
    }

    // --section previously accepted seven values but only ever honoured
    // `abstract`; every other value silently returned the whole document.
    #[test]
    fn extract_section_introduction_returns_content() {
        let text = extract_text(&fixture_path()).unwrap();
        let intro = extract_section(&text, "introduction");
        assert!(intro.is_some(), "should find the introduction");
        assert!(!intro.unwrap().trim().is_empty());
    }

    #[test]
    fn extract_section_stops_at_the_next_heading() {
        let text = extract_text(&fixture_path()).unwrap();
        let intro = extract_section(&text, "introduction").unwrap();
        assert!(
            !intro.to_lowercase().contains("\nreferences"),
            "introduction should not run into the references section"
        );
    }

    // extract_section is a pure string function, so exercise the generic
    // behaviour on synthetic text. The fixture PDF is a 311-character stub with
    // only an abstract and an introduction.
    const PAPER: &str = "\
Title Here
Abstract
We present a thing.
Introduction
Prior work was slow.
Methods
We used a hammer.
Results
It worked.
Conclusion
Hammers are good.
References
[1] Someone, 2020.";

    #[test]
    fn extract_section_conclusion_returns_content() {
        let s = extract_section(PAPER, "conclusion").unwrap();
        assert!(s.contains("Hammers are good"), "got: {:?}", s);
    }

    #[test]
    fn extract_section_conclusion_stops_before_references() {
        let s = extract_section(PAPER, "conclusion").unwrap();
        assert!(!s.contains("Someone, 2020"), "ran into references: {:?}", s);
    }

    #[test]
    fn extract_section_methods_returns_only_methods() {
        let s = extract_section(PAPER, "methods").unwrap();
        assert!(s.contains("hammer"), "got: {:?}", s);
        assert!(!s.contains("It worked"), "ran into results: {:?}", s);
    }

    #[test]
    fn extract_section_is_case_insensitive() {
        assert_eq!(
            extract_section(PAPER, "METHODS"),
            extract_section(PAPER, "methods")
        );
    }

    #[test]
    fn extract_section_unknown_heading_returns_none() {
        assert!(extract_section(PAPER, "acknowledgements of nonexistence").is_none());
    }

    #[test]
    fn extract_section_abstract_matches_the_generic_extractor() {
        let text = extract_text(&fixture_path()).unwrap();
        assert_eq!(
            extract_section_abstract(&text),
            extract_section(&text, "abstract")
        );
    }

    #[test]
    fn extract_text_nonexistent_file_returns_err() {
        let result = extract_text(Path::new("/nonexistent/fake.pdf"));
        assert!(result.is_err());
    }
}
