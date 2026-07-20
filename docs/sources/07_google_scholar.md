# Google Scholar

- **API类型**: HTML 爬取 (无官方 API)
- **搜索URL**: `https://scholar.google.com/scholar`
- **认证**: 无需
- **能力**: search only (**实验性**，有 captcha 风险)
- **反爬策略**: UA轮换 + 随机延迟 + 指数退避 + CAPTCHA检测

## search(args: &SearchArgs) -> SearchResult

```
papers = []
start = 0
results_per_page = min(10, args.limit)

// UA 池
browsers = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
]

while papers.len() < args.limit:
    params = { q: build_query(args), start: start + args.offset, hl: "en", as_sdt: "0,5" }

    // 重试循环 (最多3次):
    for attempt in 0..3:
        user_agent = random_choice(browsers)       // 随机切换 UA
        sleep(random_uniform(1.0, 2.5))            // 模拟人类延迟

        response = ureq::get(SCHOLAR_URL)
            .set("User-Agent", user_agent)
            .query_pairs(params)
            .call()

        if response.status() == 200: break
        if response.status() in [403, 429, 503]:
            wait = min(30, 2.0 * 2^attempt + random_uniform(0, 0.5))
            sleep(wait)                            // 指数退避
            continue
        else: break  // 不可重试的状态码

    if response.status() != 200: break

    // 使用 scraper crate 解析 HTML
    doc = scraper::Html::parse_document(response.body)

    // CAPTCHA 检测
    if doc.select("form#gs_captcha_f").next().is_some()
       || doc.select("input[name=captcha]").next().is_some():
        eprintln!("scholar: bot detection triggered, results may be incomplete")
        break

    results = doc.select("div.gs_ri")
    if results.is_empty(): break

    for item in results:
        paper = parse_paper(item)
        if paper: papers.push(paper)
        if papers.len() >= args.limit: break

    start += results_per_page

// 不支持的 filter 发 stderr 警告
if args.sort != Relevance:  warn("scholar: --sort not supported, ignored")
if args.field:              warn("scholar: --field not supported, ignored")
if args.open_access:        warn("scholar: --open-access not supported, ignored")
if args.peer_reviewed:      warn("scholar: --peer-reviewed not supported, ignored")

return SearchResult { results: papers[..args.limit], ... }
```

### build_query(args)

```
query = args.query
if args.author: query = "author:\"{args.author}\" {query}"
// --after/--before/--year: Google Scholar 支持 as_ylo/as_yhi URL 参数
// 但此处简化为修改 query，也可映射到 URL params
return query
```

### parse_paper(item) -> Option<Paper>

```
title_elem = item.select("h3.gs_rt")
info_elem = item.select("div.gs_a")
abstract_elem = item.select("div.gs_rs")

title = title_elem.text().replace("[PDF]", "").replace("[HTML]", "").trim()
link = title_elem.select("a[href]")
url = link.attr("href")

info_text = info_elem.text()
authors = info_text.split("-")[0].split(",").map(trim)
year = extract_year(info_text)  // 匹配 1900~当前年的4位数字

doi = extract_doi(url) || extract_doi(title) || extract_doi(info_text)
      || extract_doi(abstract_elem.text())

return Some(Paper {
    id: "gs_{hash(url)}",
    title: title,
    authors: authors,
    abstract_text: Some(abstract_elem.text()),
    year: year,
    doi: doi,
    url: Some(url),
    pdf_url: None,                 // Scholar 不直接提供 PDF URL
    venue: None,
    citations: None,               // 不可靠，输出 null
    fields: [],
    open_access: None,
    source: "scholar",             // 注意: 不是 "google_scholar"
})
```

## download / read

```
return Err(CliError::NotSupported {
    source: "scholar",
    action: "download" / "read",
    hint: "Google Scholar doesn't provide direct PDF access. Use DOI with another source.",
})
```
