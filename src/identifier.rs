/// The type of identifier detected from user input.
#[derive(Debug, Clone, PartialEq)]
pub enum IdType {
    /// arXiv new format: 2301.08745, 2301.08745v2
    Arxiv,
    /// arXiv old format: hep-th/9711200
    ArxivOld,
    /// DOI: 10.xxxx/...
    Doi,
    /// PMC ID: PMC7318926
    Pmc,
    /// PubMed ID: PMID:12345678 or bare 7-8 digit number
    Pmid,
    /// Semantic Scholar ID: S2:abc123
    S2,
    /// URL (will be routed by domain)
    Url,
    /// Unknown format
    Unknown,
}

/// Detect the identifier type from a string.
pub fn detect_id_type(input: &str) -> IdType {
    // arXiv new format: YYMM.NNNNN(vN)
    if is_arxiv_new(input) {
        return IdType::Arxiv;
    }
    // arXiv old format: category/NNNNNNN
    if is_arxiv_old(input) {
        return IdType::ArxivOld;
    }
    // DOI: 10.NNNN/...
    if is_doi(input) {
        return IdType::Doi;
    }
    // PMC ID: PMC followed by digits
    if input.starts_with("PMC") && input.len() > 3 && input[3..].chars().all(|c| c.is_ascii_digit()) {
        return IdType::Pmc;
    }
    // PMID with prefix: PMID:NNNNNNN(N)
    if input.starts_with("PMID:") {
        let digits = &input[5..];
        if (digits.len() == 7 || digits.len() == 8) && digits.chars().all(|c| c.is_ascii_digit()) {
            return IdType::Pmid;
        }
    }
    // Semantic Scholar: S2:<hex>
    if input.starts_with("S2:") && input.len() > 3 && input[3..].chars().all(|c| c.is_ascii_hexdigit()) {
        return IdType::S2;
    }
    // URL
    if input.starts_with("http://") || input.starts_with("https://") {
        return IdType::Url;
    }
    // Bare 7-8 digit number (possible PMID)
    if (input.len() == 7 || input.len() == 8) && input.chars().all(|c| c.is_ascii_digit()) {
        return IdType::Pmid;
    }
    IdType::Unknown
}

fn is_arxiv_new(input: &str) -> bool {
    let (base, _) = match input.rfind('v') {
        Some(pos) if input[pos + 1..].chars().all(|c| c.is_ascii_digit())
            && !input[pos + 1..].is_empty() =>
        {
            (&input[..pos], &input[pos..])
        }
        _ => (input, ""),
    };
    let Some(dot) = base.find('.') else {
        return false;
    };
    let prefix = &base[..dot];
    let suffix = &base[dot + 1..];
    prefix.len() == 4
        && prefix.chars().all(|c| c.is_ascii_digit())
        && (suffix.len() == 4 || suffix.len() == 5)
        && suffix.chars().all(|c| c.is_ascii_digit())
}

fn is_doi(input: &str) -> bool {
    if !input.starts_with("10.") {
        return false;
    }
    let rest = &input[3..];
    let Some(slash) = rest.find('/') else {
        return false;
    };
    let prefix = &rest[..slash];
    let suffix = &rest[slash + 1..];
    prefix.len() >= 4
        && prefix.chars().all(|c| c.is_ascii_digit())
        && !suffix.is_empty()
        && !suffix.chars().any(|c| c.is_whitespace())
}

fn is_arxiv_old(input: &str) -> bool {
    let Some(slash) = input.find('/') else {
        return false;
    };
    let cat = &input[..slash];
    let num = &input[slash + 1..];
    !cat.is_empty()
        && cat.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        && num.len() == 7
        && num.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_arxiv_new_format() {
        assert_eq!(detect_id_type("2301.08745"), IdType::Arxiv);
    }

    #[test]
    fn detect_arxiv_new_format_with_version() {
        assert_eq!(detect_id_type("2301.08745v2"), IdType::Arxiv);
    }

    #[test]
    fn detect_arxiv_old_format() {
        assert_eq!(detect_id_type("hep-th/9711200"), IdType::ArxivOld);
    }

    #[test]
    fn detect_doi() {
        assert_eq!(detect_id_type("10.1038/nature12373"), IdType::Doi);
    }

    #[test]
    fn detect_pmc() {
        assert_eq!(detect_id_type("PMC7318926"), IdType::Pmc);
    }

    #[test]
    fn detect_pmid_with_prefix() {
        assert_eq!(detect_id_type("PMID:33475315"), IdType::Pmid);
    }

    #[test]
    fn detect_pmid_bare_digits() {
        assert_eq!(detect_id_type("33475315"), IdType::Pmid);
    }

    #[test]
    fn detect_url_arxiv() {
        assert_eq!(detect_id_type("https://arxiv.org/abs/2301.08745"), IdType::Url);
    }

    #[test]
    fn detect_url_doi() {
        assert_eq!(detect_id_type("https://doi.org/10.1038/nature12373"), IdType::Url);
    }

    #[test]
    fn detect_semantic_scholar() {
        assert_eq!(detect_id_type("S2:abc123def"), IdType::S2);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_id_type("some random string"), IdType::Unknown);
    }
}
