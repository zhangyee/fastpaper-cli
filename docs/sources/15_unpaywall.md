# Unpaywall

- **API类型**: REST JSON
- **基础URL**: `https://api.unpaywall.org/v2`
- **认证**: **需要** `UNPAYWALL_EMAIL` (免费, API 条款要求使用者提供联系邮箱)
- **能力**: search (DOI 查找) only
- **特点**: 不是传统搜索引擎, 只按 DOI 查找 OA 链接

## search(args: &SearchArgs) -> SearchResult

```
// 检查必需环境变量
email = env::var("UNPAYWALL_EMAIL")
if email.is_err():
    return Err(CliError::Permission(
        "source 'unpaywall' requires UNPAYWALL_EMAIL to be set.\n\
         Hint: export UNPAYWALL_EMAIL=\"your@email.com\"\n\
         Unpaywall API requires a contact email."
    ))

// query 被当作 DOI 处理
doi = extract_doi(args.query) || (args.query if args.query.starts_with("10."))
if !doi:
    eprintln!("unpaywall: query must be a DOI. Unpaywall only supports DOI lookup, not keyword search.")
    return SearchResult { results: [], ... }

// 获取 DOI 记录
data = fetch_doi_record(doi, email)
//   GET "{BASE_URL}/{doi}", params={email: email}, timeout=20s
//   指数退避重试 (最多3次)
//   404 → return empty
//   422 → email 无效

if !data: return SearchResult { results: [], ... }

// 解析结果
title = data["title"]
authors = data["z_authors"].map(|a| "{a.given} {a.family}".trim())
year = data["year"]

best_location = data["best_oa_location"]
landing_url = best_location["url"] || data["doi_url"] || "https://doi.org/{doi}"

// PDF URL: 优先 best_oa_location, 降级遍历 oa_locations[]
pdf_url = best_location["url_for_pdf"]
if !pdf_url:
    for location in data["oa_locations"]:
        candidate = location["url_for_pdf"] || location["url"]
        if candidate:
            pdf_url = Some(candidate)
            break

paper = Paper {
    id: "unpaywall:{doi}",
    title: title,
    authors: authors,
    abstract_text: None,         // Unpaywall 不提供摘要
    year: year,
    doi: Some(doi),
    url: Some(landing_url),
    pdf_url: pdf_url,
    venue: data["journal_name"],
    citations: None,
    fields: [],
    open_access: Some(data["is_oa"]),
    source: "unpaywall",
}

return SearchResult { results: [paper], total: Some(1), ... }
```

## download / read

```
return Err(CliError::NotSupported {
    source: "unpaywall",
    action: "download" / "read",
    hint: "Unpaywall is an OA resolver, not a content host. Use the pdf_url from search results:\n  fastpaper download semantic <DOI>",
})
```
