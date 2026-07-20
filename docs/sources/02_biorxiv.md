# BioRxiv

- **API类型**: REST JSON
- **基础URL**: `https://api.biorxiv.org/details/biorxiv`
- **认证**: 无需
- **能力**: search + download + read
- **限制**: API 按**类别+日期范围**浏览，不支持关键词全文搜索。query 被映射为 category 参数，关键词过滤在本地执行。

## search(args: &SearchArgs) -> SearchResult

```
// 日期范围: 默认最近30天，可通过 --after/--before 控制
end_date = args.before || today().format("%Y-%m-%d")
start_date = args.after || (today() - 30 days).format("%Y-%m-%d")

// --field 映射为 biorxiv category; 如果 query 本身像 category 名也用作 category
category = args.field || query_to_category(args.query)
//   query_to_category: 将 query 转为小写下划线格式尝试匹配已知 category
//   无法匹配时 category = None，不传 category 参数（获取所有类别）

papers = []
cursor = 0

loop:
    url = "{BASE_URL}/{start_date}/{end_date}/{cursor}"
    if category: url += "?category={category}"

    // 指数退避重试
    response = request_with_retry(url, timeout=30s)
    data = response.json()
    collection = data["collection"]

    for item in collection:
        paper = Paper {
            id: item["doi"],
            title: item["title"],
            authors: item["authors"].split("; "),
            abstract_text: Some(item["abstract"]),
            url: Some("https://www.biorxiv.org/content/{doi}v{version}"),
            pdf_url: Some("https://www.biorxiv.org/content/{doi}v{version}.full.pdf"),
            year: Some(parse(item["date"], "%Y-%m-%d").year),
            source: "biorxiv",
            fields: [item["category"]],
            doi: Some(item["doi"]),
            venue: Some("bioRxiv preprint"),
            citations: None,
            open_access: Some(true),
        }

        // 本地关键词过滤 (因 API 不支持全文搜索)
        if !matches_query(paper, args.query): continue

        // 本地过滤: --year, --author
        if args.year && paper.year != Some(args.year): continue
        if args.author && !paper.authors.any(|a| a.contains(args.author)): continue

        papers.push(paper)

    if collection.len() < 100: break  // 没有更多结果
    cursor += 100
    if papers.len() >= args.limit: break

// 不支持的 filter 发 stderr 警告
if args.sort != Relevance:  warn("biorxiv: --sort only supports date ordering via API, ignored")
if args.peer_reviewed:      warn("biorxiv: --peer-reviewed not supported, ignored")
if args.open_access:        warn("biorxiv: --open-access ignored (all bioRxiv papers are open access)")

return SearchResult { results: papers[..args.limit], ... }
```

### matches_query(paper, query) -> bool

```
// 在 title + abstract 中搜索 query 关键词 (case-insensitive)
let lower_query = query.to_lowercase()
let text = format!("{} {}", paper.title, paper.abstract_text.unwrap_or_default()).to_lowercase()
lower_query.split_whitespace().all(|word| text.contains(word))
```

## download(identifier) -> DownloadResult

```
// identifier 就是 DOI
pdf_url = "https://www.biorxiv.org/content/{identifier}v1.full.pdf"
// 带浏览器 UA，指数退避重试
response = request_with_retry(pdf_url, headers={User-Agent: browser_ua}, timeout=30s)
return DownloadResult { content: response.body, ... }
```

## read(identifier) -> ReadResult

```
// 下载 PDF → pdf_oxide 提取文本 (同 ArXiv)
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
