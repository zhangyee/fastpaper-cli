use super::Paper;

// 百度学术新版 JSON 搜索接口(未公开站内接口,实验性数据源)。
// 调研:docs 见 kydog-lab/百度学术新版JSON搜索接口调研与实现.md

// 去除 <em> 等搜索高亮标签并解码常见 HTML 实体。title 和 abstract 都可能带高亮。
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

// saveList 里混有 PDF 直链和第三方落地页;只取首个看起来是 PDF 的 http 链接。
fn pick_pdf_url(save_list: &serde_json::Value) -> Option<String> {
    save_list.as_array()?.iter().find_map(|s| {
        s["url"]
            .as_str()
            .filter(|u| u.starts_with("http") && u.to_lowercase().contains(".pdf"))
            .map(|u| u.to_string())
    })
}

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
        let title = strip_html(item["title"].as_str().unwrap_or(""));
        if id.is_empty() || title.is_empty() {
            continue;
        }

        let authors: Vec<String> = item["authors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        let show = a["showName"].as_str().unwrap_or("");
                        let name = if show.is_empty() {
                            a["name"].as_str().unwrap_or("")
                        } else {
                            show
                        };
                        (!name.is_empty()).then(|| name.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        let abstract_text = Some(strip_html(item["abstract"].as_str().unwrap_or("")))
            .filter(|s| !s.is_empty());

        let doi = item["doi"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let venue = ["journalName", "publisher"]
            .iter()
            .find_map(|k| item["publishInfo"][*k].as_str().filter(|s| !s.is_empty()))
            .map(|s| s.to_string());

        let url = format!(
            "https://xueshu.baidu.com/usercenter/paper/show?paperid={}&site=xueshu_se",
            id
        );

        papers.push(Paper {
            pdf_url: pick_pdf_url(&item["saveList"]),
            url: Some(url),
            year: item["publishYear"]
                .as_u64()
                .filter(|y| *y > 0)
                .map(|y| y as u16),
            citations: item["cited"].as_u64().map(|n| n as u32),
            id,
            title,
            authors,
            abstract_text,
            doi,
            venue,
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

    #[test]
    fn parse_strips_em_tags_from_title() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[2].title, "Scale Attention Mechanism");
        assert_eq!(papers[8].title, "ATTENTION MECHANISM");
        for p in &papers {
            assert!(!p.title.contains('<'), "title not stripped: {}", p.title);
        }
    }

    #[test]
    fn parse_authors_use_show_name() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].authors, vec!["A Bernardino", "J Santos-Victor"]);
    }

    #[test]
    fn parse_chinese_authors_and_show_name_fallback() {
        // 中文作者 + showName 为空回退 name + 空作者被过滤
        let body = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"paperList":[
            {"paperId":"abc","title":"ChatModeler:基于<em>大语言模型</em>的方法",
             "authors":[{"name":"张三","showName":"张三(教授)","affiliate":""},
                        {"name":"李四","showName":"","affiliate":""},
                        {"name":"","showName":"","affiliate":""}]}
        ]}}}"#;
        let papers = parse_search_response(body).unwrap();
        assert_eq!(papers[0].title, "ChatModeler:基于大语言模型的方法");
        assert_eq!(papers[0].authors, vec!["张三(教授)", "李四"]);
    }

    #[test]
    fn parse_empty_doi_becomes_none() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].doi, None);
        assert_eq!(papers[1].doi.as_deref(), Some("10.1007/978-1-4615-0111-4"));
    }

    #[test]
    fn parse_year_and_citations() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].year, Some(2002));
        assert_eq!(papers[0].citations, Some(15));
    }

    #[test]
    fn parse_empty_venue_becomes_none() {
        // 第 1 篇 journalName 和 publisher 都是空串
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(papers[0].venue, None);
    }

    #[test]
    fn parse_venue_falls_back_to_publisher() {
        let body = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"paperList":[
            {"paperId":"a1","title":"T1","publishInfo":{"publisher":"P","journalName":"J","journalId":""}},
            {"paperId":"a2","title":"T2","publishInfo":{"publisher":"P","journalName":"","journalId":""}}
        ]}}}"#;
        let papers = parse_search_response(body).unwrap();
        assert_eq!(papers[0].venue.as_deref(), Some("J"));
        assert_eq!(papers[1].venue.as_deref(), Some("P"));
    }

    #[test]
    fn parse_url_points_to_xueshu_paper_page() {
        let papers = parse_search_response(FIXTURE).unwrap();
        assert_eq!(
            papers[0].url.as_deref(),
            Some("https://xueshu.baidu.com/usercenter/paper/show?paperid=5a4afe9b6df8b07138fb6bf9b275251f&site=xueshu_se")
        );
    }

    #[test]
    fn parse_pdf_url_picks_first_pdf_in_save_list() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let pdf = papers[0].pdf_url.as_deref().unwrap();
        assert!(
            pdf.contains("productFlyer-CN_978-0-306-47427-9.pdf"),
            "got {pdf}"
        );
    }

    #[test]
    fn parse_pdf_url_none_when_save_list_has_no_pdf() {
        let body = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"paperList":[
            {"paperId":"a1","title":"T1","saveList":[{"url":"http://example.com/doc/detail?id=1","anchor":"x","type":0,"size":""}]},
            {"paperId":"a2","title":"T2"}
        ]}}}"#;
        let papers = parse_search_response(body).unwrap();
        assert_eq!(papers[0].pdf_url, None);
        assert_eq!(papers[1].pdf_url, None);
    }

    #[test]
    fn parse_abstract_stripped_and_nonempty() {
        let papers = parse_search_response(FIXTURE).unwrap();
        let a = papers[0].abstract_text.as_deref().unwrap();
        assert!(a.contains("Visual Attention"));
        for p in &papers {
            if let Some(ref a) = p.abstract_text {
                assert!(!a.contains("<em>"), "abstract not stripped: {}", p.id);
            }
        }
    }

    #[test]
    fn parse_minimal_item_with_missing_fields_ok() {
        // null、空串、缺字段都不能导致解析失败(spec 12.1)
        let body = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"paperList":[
            {"paperId":"only-id","title":"Only Title","abstract":null,"publishYear":null,
             "cited":null,"doi":null,"authors":null,"publishInfo":null,"saveList":null}
        ]}}}"#;
        let papers = parse_search_response(body).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.abstract_text, None);
        assert_eq!(p.year, None);
        assert_eq!(p.citations, None);
        assert_eq!(p.doi, None);
        assert!(p.authors.is_empty());
        assert_eq!(p.venue, None);
        assert_eq!(p.pdf_url, None);
    }

    #[test]
    fn strip_html_removes_tags_and_decodes_entities() {
        assert_eq!(strip_html("Scale <em>Attention</em>"), "Scale Attention");
        assert_eq!(strip_html("A &amp; B &lt;C&gt;"), "A & B <C>");
        assert_eq!(strip_html("  padded  "), "padded");
        assert_eq!(strip_html("no tags"), "no tags");
    }
}
