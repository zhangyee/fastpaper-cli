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
    if regex_matches(input, r"^\d{4}\.\d{4,5}(v\d+)?$") {
        return IdType::Arxiv;
    }
    // arXiv old format: category/NNNNNNN
    if regex_matches(input, r"^[a-z-]+/\d{7}$") {
        return IdType::ArxivOld;
    }
    // DOI
    if regex_matches(input, r"^10\.\d{4,}/\S+$") {
        return IdType::Doi;
    }
    // PMC ID
    if regex_matches(input, r"^PMC\d+$") {
        return IdType::Pmc;
    }
    // Semantic Scholar
    if input.starts_with("S2:") {
        return IdType::S2;
    }
    // PMID with prefix
    if regex_matches(input, r"^PMID:\d{7,8}$") {
        return IdType::Pmid;
    }
    // URL
    if input.starts_with("http://") || input.starts_with("https://") {
        return IdType::Url;
    }
    // Bare 7-8 digit number (possible PMID)
    if regex_matches(input, r"^\d{7,8}$") {
        return IdType::Pmid;
    }
    IdType::Unknown
}

fn regex_matches(input: &str, pattern: &str) -> bool {
    // Simple regex matching without pulling in the regex crate.
    // For now, we do basic string checks. Will be replaced with
    // proper regex if needed.
    use std::sync::OnceLock;

    // We'll use a minimal approach: compile-time patterns are simple
    // enough to check manually, but for correctness we parse the
    // pattern structure.
    match pattern {
        r"^\d{4}\.\d{4,5}(v\d+)?$" => {
            let bytes = input.as_bytes();
            if bytes.len() < 9 {
                return false;
            }
            let dot_pos = match input.find('.') {
                Some(p) => p,
                None => return false,
            };
            if dot_pos != 4 {
                return false;
            }
            if !bytes[..4].iter().all(|b| b.is_ascii_digit()) {
                return false;
            }
            let rest = &input[5..];
            // strip optional vN suffix
            let (digits, _suffix) = if let Some(v_pos) = rest.rfind('v') {
                let after_v = &rest[v_pos + 1..];
                if after_v.chars().all(|c| c.is_ascii_digit()) && !after_v.is_empty() {
                    (&rest[..v_pos], &rest[v_pos..])
                } else {
                    (rest, "")
                }
            } else {
                (rest, "")
            };
            digits.len() >= 4
                && digits.len() <= 5
                && digits.chars().all(|c| c.is_ascii_digit())
        }
        r"^[a-z-]+/\d{7}$" => {
            if let Some(slash) = input.find('/') {
                let cat = &input[..slash];
                let num = &input[slash + 1..];
                !cat.is_empty()
                    && cat.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                    && num.len() == 7
                    && num.chars().all(|c| c.is_ascii_digit())
            } else {
                false
            }
        }
        r"^10\.\d{4,}/\S+$" => {
            input.starts_with("10.")
                && input.len() > 7
                && input[3..].find('/').map_or(false, |slash_offset| {
                    let prefix = &input[3..3 + slash_offset];
                    prefix.len() >= 4
                        && prefix.chars().all(|c| c.is_ascii_digit())
                        && input[3 + slash_offset + 1..].len() > 0
                        && !input[3 + slash_offset + 1..]
                            .chars()
                            .any(|c| c.is_whitespace())
                })
        }
        r"^PMC\d+$" => {
            input.starts_with("PMC")
                && input.len() > 3
                && input[3..].chars().all(|c| c.is_ascii_digit())
        }
        r"^PMID:\d{7,8}$" => {
            input.starts_with("PMID:")
                && {
                    let digits = &input[5..];
                    (digits.len() == 7 || digits.len() == 8)
                        && digits.chars().all(|c| c.is_ascii_digit())
                }
        }
        r"^\d{7,8}$" => {
            (input.len() == 7 || input.len() == 8)
                && input.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}
