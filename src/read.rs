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
    "conclusions",
    "references",
    "bibliography",
    "acknowledgements",
    "acknowledgments",
    "appendix",
];

fn normalize_heading(line: &str) -> String {
    let heading = line.trim().trim_end_matches(':').to_lowercase();
    let mut parts = heading.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    let rest = parts.next();
    if first.chars().any(|c| c.is_ascii_digit())
        && first
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
    {
        rest.unwrap_or("").trim().to_string()
    } else {
        heading
    }
}

fn section_aliases(section: &str) -> Option<&'static [&'static str]> {
    match section {
        "abstract" => Some(&["abstract"]),
        "introduction" => Some(&["introduction"]),
        "methods" => Some(&["methods", "methodology", "materials and methods"]),
        "results" => Some(&["results"]),
        "discussion" => Some(&["discussion"]),
        "conclusion" => Some(&["conclusion", "conclusions"]),
        "references" => Some(&["references", "bibliography"]),
        _ => None,
    }
}

/// Extract a named section from PDF text using standalone section headings.
pub fn extract_section(full_text: &str, section: &str) -> Option<String> {
    let aliases = section_aliases(section)?;
    let lines: Vec<&str> = full_text.lines().collect();
    let start = lines
        .iter()
        .position(|line| aliases.contains(&normalize_heading(line).as_str()))?
        + 1;
    let end = lines[start..]
        .iter()
        .position(|line| SECTION_HEADINGS.contains(&normalize_heading(line).as_str()))
        .map(|offset| start + offset)
        .unwrap_or(lines.len());
    let section = lines[start..end].join("\n");
    let section = section.trim();
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
        let abstract_text = extract_section(&text, "abstract");
        assert!(abstract_text.is_some(), "should find abstract section");
        let abs = abstract_text.unwrap();
        assert!(!abs.is_empty());
    }

    #[test]
    fn extract_section_abstract_does_not_contain_introduction() {
        let text = extract_text(&fixture_path()).unwrap();
        let abs = extract_section(&text, "abstract").unwrap();
        let lower = abs.to_lowercase();
        assert!(
            !lower.contains("introduction"),
            "abstract should not contain introduction heading, got: {}",
            abs
        );
    }

    #[test]
    fn extract_section_supports_numbered_heading_aliases() {
        let text = "1 Introduction\nintro body\n2 Materials and Methods\nmethod body\n3 Results\nresult body";

        let methods = extract_section(text, "methods").unwrap();

        assert_eq!(methods, "method body");
    }

    #[test]
    fn extract_text_from_bytes_works() {
        let bytes = std::fs::read(fixture_path()).unwrap();
        let text = extract_text_from_bytes(&bytes).unwrap();
        assert!(!text.is_empty());
    }

    #[test]
    fn extract_text_nonexistent_file_returns_err() {
        let result = extract_text(Path::new("/nonexistent/fake.pdf"));
        assert!(result.is_err());
    }
}
