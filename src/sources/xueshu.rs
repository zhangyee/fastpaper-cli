use std::time::Duration;

use super::Paper;

// 百度学术新版 JSON 搜索接口(未公开站内接口,实验性数据源)。
// 调研:docs 见 kydog-lab/百度学术新版JSON搜索接口调研与实现.md

// 业务码 7350001 = 需要交互式验证码。CLI 不自动处理、不模拟验证(spec 3.4/13)。
const CODE_CAPTCHA: i64 = 7350001;

// Acs-Token 回退值:百度学术新版前端在反风险 SDK(paris)未初始化时发送的静态字符串,
// 后端当前(2026-07 实测)接受它。来源:前端主包 Cnu8PP3d.js。若失效表现为 7350001。
const ACS_TOKEN_FALLBACK: &str = "parisInstance is not ready";
const PAGE_SIZE: u32 = 10;
const TIMEOUT: Duration = Duration::from_secs(30);

const USER_AGENT: &str = concat!(
    "fastpaper-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/zhangyee/fastpaper-cli)"
);

fn fetch_page(base_url: &str, query: &str, pn: u32) -> Result<String, String> {
    let encoded = super::encode_query(query);
    let url = format!(
        "{}/search/api/search?wd={}&pn={}&skipStrategy=0",
        base_url, encoded, pn
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let resp = agent
        .get(&url)
        .header("Acs-Token", ACS_TOKEN_FALLBACK)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json, text/plain, */*")
        .call()
        .map_err(|e| format!("HTTP error: {e}"))?;
    let status = resp.status().as_u16();
    match status {
        // 206 携带 7350001 验证码 JSON;读 body 交给解析层给出明确错误
        200 | 206 => resp
            .into_body()
            .read_to_string()
            .map_err(|e| format!("read body: {e}")),
        403 => Err("Baidu Xueshu blocked the request (HTTP 403 security check)".to_string()),
        429 => Err("Baidu Xueshu rate limited (HTTP 429). Try again later.".to_string()),
        s => Err(format!("HTTP {s}")),
    }
}

/// Search Baidu Xueshu (experimental, undocumented JSON API). Serial pagination, 10 per page.
pub fn search(base_url: &str, q: &super::SearchQuery) -> Result<Vec<Paper>, String> {
    let (query, max_results) = (q.query.as_str(), q.offset + q.limit);
    let mut papers: Vec<Paper> = Vec::new();
    let mut pn = 0u32;
    while (papers.len() as u32) < max_results {
        let body = fetch_page(base_url, query, pn)?;
        let page = parse_search_response(&body, q.patents)?;
        if page.is_empty() {
            break;
        }
        papers.extend(page);
        pn += PAGE_SIZE;
    }
    papers.truncate(max_results as usize);
    let papers: Vec<Paper> = papers.into_iter().skip(q.offset as usize).collect();
    Ok(papers)
}

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

// Baidu overloads its `doi` field as a general external identifier: alongside
// real DOIs it returns patent numbers ("CN103232069 A") and bare repository
// URLs. Passing those on as a DOI breaks every consumer that resolves one
// against Crossref or Unpaywall, so anything not shaped like a DOI is dropped.
fn doi_of(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| crate::identifier::detect_id_type(s) == crate::identifier::IdType::Doi)
        .map(|s| s.to_string())
}

// Baidu's numeric document type. 5 is a patent — verified 2026-07-28: those
// records carry the assignee in `publisher` with no journal, while type 1
// carries a journal. Types 0 and 4 also occur and are not identified here;
// they are treated as "not a patent", which is all this needs to decide.
const TYPE_PATENT: i64 = 5;

/// Parse Baidu Xueshu JSON search response into a list of Papers.
///
/// `patents_only` splits the mixed result set: Baidu returns patents alongside
/// papers and exposes no server-side filter for it (its `filter` parameter is
/// accepted and ignored), so the split happens here, where the type code is
/// still visible. The two modes partition the response — every record lands in
/// exactly one of them.
pub fn parse_search_response(json: &str, patents_only: bool) -> Result<Vec<Paper>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))?;

    let code = root["status"]["code"].as_i64().unwrap_or(-1);
    if code == CODE_CAPTCHA {
        return Err(
            "Baidu Xueshu requires an interactive captcha (code 7350001). Not retrying; try again later or from a different network."
                .to_string(),
        );
    }
    if code != 0 {
        let msg = root["status"]["msg"].as_str().unwrap_or("unknown error");
        let logid = root["status"]["sLogid"].as_str().unwrap_or("-");
        return Err(format!("Baidu Xueshu error {code}: {msg} (sLogid {logid})"));
    }

    let list = root["data"]["paper"]["paperList"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut papers = Vec::new();
    for item in &list {
        let is_patent = item["type"].as_i64() == Some(TYPE_PATENT);
        if is_patent != patents_only {
            continue;
        }
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

        let abstract_text =
            Some(strip_html(item["abstract"].as_str().unwrap_or(""))).filter(|s| !s.is_empty());

        let doi = doi_of(item["doi"].as_str());

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

    // Baidu mixes patents (type 5) into ordinary results whether or not you
    // want them, and offers no server-side filter for it — a `filter` query
    // parameter exists but is ignored (verified 2026-07-28), so the split has
    // to happen locally, at the only point where the type code is still visible.
    #[test]
    fn patents_only_keeps_nothing_else() {
        let papers = parse_search_response(FIXTURE, true).unwrap();
        assert!(!papers.is_empty(), "fixture has a patent record");
        assert!(papers.iter().all(|p| p.title.contains("一种锂离子电池")));
    }

    #[test]
    fn patents_are_excluded_by_default() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert!(!papers.is_empty());
        assert!(
            papers.iter().all(|p| !p.title.contains("一种锂离子电池")),
            "the type-5 record must not survive"
        );
    }

    // The two modes must partition the result set: nothing lost, nothing double
    // counted.
    #[test]
    fn the_two_modes_partition_the_records() {
        let all = FIXTURE.matches("\"paperId\"").count();
        let patents = parse_search_response(FIXTURE, true).unwrap().len();
        let rest = parse_search_response(FIXTURE, false).unwrap().len();
        assert_eq!(patents + rest, all);
    }

    fn by_title_in(title: &str, patents_only: bool) -> Paper {
        parse_search_response(FIXTURE, patents_only)
            .unwrap()
            .into_iter()
            .find(|p| p.title.contains(title))
            .unwrap_or_else(|| panic!("fixture has no record titled {title}"))
    }

    fn by_title(title: &str) -> Paper {
        by_title_in(title, false)
    }

    // Baidu overloads its `doi` field as a general external identifier: it
    // holds real DOIs, but also patent numbers and bare repository URLs.
    // Passing those on as a DOI breaks anything that resolves one.
    #[test]
    fn a_patent_number_does_not_become_a_doi() {
        assert_eq!(by_title_in("一种锂离子电池正极材料", true).doi, None);
    }

    #[test]
    fn a_bare_url_does_not_become_a_doi() {
        assert_eq!(by_title("A thesis deposited").doi, None);
    }

    #[test]
    fn a_real_doi_still_comes_through() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert!(
            papers
                .iter()
                .any(|p| p.doi.as_deref() == Some("10.1109/ICPR.1988.28419")),
            "real DOIs must survive"
        );
    }

    #[test]
    fn parse_returns_ok() {
        let result = parse_search_response(FIXTURE, false);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    #[test]
    fn parse_paper_ids_not_empty() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        for p in &papers {
            assert!(!p.id.is_empty());
        }
        assert_eq!(papers[0].id, "5a4afe9b6df8b07138fb6bf9b275251f");
    }

    #[test]
    fn parse_source_is_xueshu() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        for p in &papers {
            assert_eq!(p.source, "xueshu");
        }
    }

    #[test]
    fn parse_strips_em_tags_from_title() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert_eq!(papers[2].title, "Scale Attention Mechanism");
        assert_eq!(papers[8].title, "ATTENTION MECHANISM");
        for p in &papers {
            assert!(!p.title.contains('<'), "title not stripped: {}", p.title);
        }
    }

    #[test]
    fn parse_authors_use_show_name() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
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
        let papers = parse_search_response(body, false).unwrap();
        assert_eq!(papers[0].title, "ChatModeler:基于大语言模型的方法");
        assert_eq!(papers[0].authors, vec!["张三(教授)", "李四"]);
    }

    #[test]
    fn parse_empty_doi_becomes_none() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert_eq!(papers[0].doi, None);
        assert_eq!(papers[1].doi.as_deref(), Some("10.1007/978-1-4615-0111-4"));
    }

    #[test]
    fn parse_year_and_citations() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert_eq!(papers[0].year, Some(2002));
        assert_eq!(papers[0].citations, Some(15));
    }

    #[test]
    fn parse_empty_venue_becomes_none() {
        // 第 1 篇 journalName 和 publisher 都是空串
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert_eq!(papers[0].venue, None);
    }

    #[test]
    fn parse_venue_falls_back_to_publisher() {
        let body = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"paperList":[
            {"paperId":"a1","title":"T1","publishInfo":{"publisher":"P","journalName":"J","journalId":""}},
            {"paperId":"a2","title":"T2","publishInfo":{"publisher":"P","journalName":"","journalId":""}}
        ]}}}"#;
        let papers = parse_search_response(body, false).unwrap();
        assert_eq!(papers[0].venue.as_deref(), Some("J"));
        assert_eq!(papers[1].venue.as_deref(), Some("P"));
    }

    #[test]
    fn parse_url_points_to_xueshu_paper_page() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
        assert_eq!(
            papers[0].url.as_deref(),
            Some(
                "https://xueshu.baidu.com/usercenter/paper/show?paperid=5a4afe9b6df8b07138fb6bf9b275251f&site=xueshu_se"
            )
        );
    }

    #[test]
    fn parse_pdf_url_picks_first_pdf_in_save_list() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
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
        let papers = parse_search_response(body, false).unwrap();
        assert_eq!(papers[0].pdf_url, None);
        assert_eq!(papers[1].pdf_url, None);
    }

    #[test]
    fn parse_abstract_stripped_and_nonempty() {
        let papers = parse_search_response(FIXTURE, false).unwrap();
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
        let papers = parse_search_response(body, false).unwrap();
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

    const CAPTCHA_BODY: &str = r#"{"status":{"code":7350001,"msg":"请输入验证码"}}"#;

    #[test]
    fn parse_captcha_code_returns_clear_err() {
        let err = parse_search_response(CAPTCHA_BODY, false).unwrap_err();
        assert!(err.contains("captcha"), "got: {err}");
        assert!(err.contains("7350001"), "got: {err}");
    }

    #[test]
    fn parse_other_error_code_returns_err_with_msg() {
        let body = r#"{"status":{"code":42,"msg":"boom","sLogid":"log-1"}}"#;
        let err = parse_search_response(body, false).unwrap_err();
        assert!(err.contains("42"), "got: {err}");
        assert!(err.contains("boom"), "got: {err}");
        assert!(err.contains("log-1"), "got: {err}");
    }

    #[test]
    fn parse_empty_paper_list_returns_empty_vec() {
        let body = r#"{"status":{"code":0,"msg":"Success","sLogid":"x"},"data":{"paper":{"dispnum":0,"paperList":[]}}}"#;
        assert!(parse_search_response(body, false).unwrap().is_empty());
    }

    #[test]
    fn parse_code_zero_missing_data_returns_empty_vec() {
        let body = r#"{"status":{"code":0,"msg":"Success"}}"#;
        assert!(parse_search_response(body, false).unwrap().is_empty());
    }

    #[test]
    fn parse_garbage_returns_parse_err() {
        let err = parse_search_response("<html>not json</html>", false).unwrap_err();
        assert!(err.contains("JSON parse error"), "got: {err}");
    }

    #[test]
    fn strip_html_removes_tags_and_decodes_entities() {
        assert_eq!(strip_html("Scale <em>Attention</em>"), "Scale Attention");
        assert_eq!(strip_html("A &amp; B &lt;C&gt;"), "A & B <C>");
        assert_eq!(strip_html("  padded  "), "padded");
        assert_eq!(strip_html("no tags"), "no tags");
    }

    #[test]
    fn search_returns_papers() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let papers = search(&server.url(), &crate::sources::SearchQuery::simple("test", 5)).unwrap();
        assert_eq!(papers.len(), 5, "should truncate to max_results");
        mock.assert();
    }

    #[test]
    fn search_request_path_and_params() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock(
                "GET",
                mockito::Matcher::Regex(
                    r"/search/api/search\?wd=attention\+mechanism&pn=0&skipStrategy=0".to_string(),
                ),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("attention mechanism", 3));
        mock.assert();
    }

    #[test]
    fn search_sends_acs_token_fallback_header() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("acs-token", "parisInstance is not ready")
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_sends_fastpaper_user_agent() {
        // 实测:curl 默认 UA 会触发 7350001;必须带项目 UA
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header(
                "user-agent",
                mockito::Matcher::Regex(r"^fastpaper-cli/\d+\.\d+\.\d+ \(\+https://".to_string()),
            )
            .with_status(200)
            .with_body(FIXTURE)
            .create();
        let _ = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3));
        mock.assert();
    }

    #[test]
    fn search_stops_after_first_page_when_satisfied() {
        let mut server = mockito::Server::new();
        let m1 = server
            .mock("GET", mockito::Matcher::Regex("pn=0".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();
        let m2 = server
            .mock("GET", mockito::Matcher::Regex("pn=10".to_string()))
            .expect(0)
            .create();
        let papers = search(&server.url(), &crate::sources::SearchQuery::simple("test", 10)).unwrap();
        assert_eq!(papers.len(), 10);
        m1.assert();
        m2.assert();
    }

    #[test]
    fn search_paginates_with_pn_10_for_second_page() {
        let mut server = mockito::Server::new();
        let m1 = server
            .mock("GET", mockito::Matcher::Regex("pn=0".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();
        let m2 = server
            .mock("GET", mockito::Matcher::Regex("pn=10".to_string()))
            .with_status(200)
            .with_body(FIXTURE)
            .expect(1)
            .create();
        let papers = search(&server.url(), &crate::sources::SearchQuery::simple("test", 15)).unwrap();
        assert_eq!(papers.len(), 15, "should truncate 20 collected to 15");
        m1.assert();
        m2.assert();
    }

    #[test]
    fn search_stops_on_empty_page() {
        let empty = r#"{"status":{"code":0,"msg":"Success"},"data":{"paper":{"dispnum":0,"paperList":[]}}}"#;
        let mut server = mockito::Server::new();
        let m1 = server
            .mock("GET", mockito::Matcher::Regex("pn=0".to_string()))
            .with_status(200)
            .with_body(empty)
            .expect(1)
            .create();
        let m2 = server
            .mock("GET", mockito::Matcher::Regex("pn=10".to_string()))
            .expect(0)
            .create();
        let papers = search(&server.url(), &crate::sources::SearchQuery::simple("test", 30)).unwrap();
        assert!(papers.is_empty());
        m1.assert();
        m2.assert();
    }

    #[test]
    fn search_206_captcha_body_returns_captcha_err() {
        // 真实观测:验证码响应是 HTTP 206 + 7350001 JSON
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(206)
            .with_body(CAPTCHA_BODY)
            .create();
        let err = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3)).unwrap_err();
        assert!(err.contains("captcha"), "got: {err}");
    }

    #[test]
    fn search_403_returns_blocked_err() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .create();
        let err = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3)).unwrap_err();
        assert!(err.contains("403"), "got: {err}");
    }

    #[test]
    fn search_429_returns_rate_limit_err() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(429)
            .create();
        let err = search(&server.url(), &crate::sources::SearchQuery::simple("test", 3)).unwrap_err();
        assert!(err.contains("rate limit"), "got: {err}");
    }
}
