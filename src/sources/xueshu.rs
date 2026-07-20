use super::Paper;

// 百度学术新版 JSON 搜索接口(未公开站内接口,实验性数据源)。
// 调研:docs 见 kydog-lab/百度学术新版JSON搜索接口调研与实现.md

/// Parse Baidu Xueshu JSON search response into a list of Papers.
pub fn parse_search_response(json: &str) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let code = root["status"]["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        let msg = root["status"]["msg"].as_str().unwrap_or("unknown error");
        return Err(format!("Baidu Xueshu error {code}: {msg}"));
    }

    let list = root["data"]["paper"]["paperList"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut papers = Vec::new();
    for item in &list {
        let id = item["paperId"].as_str().unwrap_or("").to_string();
        let title = item["title"].as_str().unwrap_or("").to_string();
        if id.is_empty() || title.is_empty() {
            continue;
        }
        papers.push(Paper {
            id,
            title,
            authors: Vec::new(),
            abstract_text: None,
            year: None,
            doi: None,
            url: None,
            pdf_url: None,
            venue: None,
            citations: None,
            fields: Vec::new(),
            open_access: None,
            source: "xueshu".to_string(),
        });
    }
    Ok(papers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/xueshu_search.json");

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn parse_returns_ten_papers() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers.len(), 10);
    }

    #[test]
    fn parse_paper_ids_not_empty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert!(!p.id.is_empty());
        }
        assert_eq!(papers[0].id, "5a4afe9b6df8b07138fb6bf9b275251f");
    }

    #[test]
    fn parse_source_is_xueshu() {
        let papers = parse_search_response(FIXTURE).unwrap();
        for p in &papers {
            assert_eq!(p.source, "xueshu");
        }
    }
}
