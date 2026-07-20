# CORE

- **API类型**: REST JSON (v3)
- **基础URL**: `https://api.core.ac.uk/v3`
- **认证**: 可选 `CORE_API_KEY` (提升 rate limit 和结果质量)
- **能力**: search + download + read
- **重试**: 429/5xx 指数退避; 401/403 时尝试去掉 key 重试

## search(args: &SearchArgs) -> SearchResult

```
params = {
    q: args.query,
    limit: min(args.limit, 100),
    offset: args.offset,
}

// 过滤参数映射
if args.year:        params["filter"] += "publishedDate>={args.year}-01-01,publishedDate<={args.year}-12-31"
if args.after:       params["filter"] += "publishedDate>={args.after}"
if args.before:      params["filter"] += "publishedDate<={args.before}"

headers = {}
if env::var("CORE_API_KEY").is_ok():
    headers["Authorization"] = "Bearer {CORE_API_KEY}"

// 指数退避重试 (最多3次)
//   429/500/502/503/504 → sleep(min(8, 2^attempt))
//   401/403 + 有 key → 去掉 key 重试一次
response = request_with_retry("{BASE_URL}/search/works", params, headers, timeout=30s)
data = response.json()
results = data["results"]
total = data["totalHits"]

for item in results:
    paper = parse_item(item)

    // 本地过滤
    if args.author && !paper.authors.any(|a| a.contains(args.author)): continue
    if args.open_access && paper.open_access != Some(true): continue

    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.sort != Relevance: warn("core: --sort not fully supported, ignored")
if args.field:             warn("core: --field not supported, ignored")
if args.peer_reviewed:     warn("core: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### parse_item(item) -> Paper

```
core_id = item["id"]
title = item["title"]
authors = item["authors"].map(|a| a["name"] or a as string)
abstract_text = item["abstract"]
doi = item["doi"] || extract_doi(abstract_text)

year = parse_year_from_date(item["publishedDate"])

url = item["url"] || "https://doi.org/{doi}"
pdf_url = None
if item["downloadUrl"] && item["downloadUrl"].ends_with(".pdf"):
    pdf_url = Some(item["downloadUrl"])
else:
    // 检查 item["fullTextUrls"] 中 .pdf 结尾的
    pdf_url = item["fullTextUrls"].find(|u| u.ends_with(".pdf"))

fields = item["subjects"].map(|s| s["name"]) || []

return Paper {
    id: core_id.to_string(),
    source: "core",
    open_access: Some(true),  // CORE 专注 OA 内容
    venue: None,
    citations: item["citationCount"],
    ...
}
```

## download(identifier) -> DownloadResult

```
// 1. 先尝试 detail 接口: GET /works/{identifier}
//    从 downloadUrl 或 fullTextUrls 找 .pdf
// 2. 无 API key 时降级: search(identifier) 取第一条有 pdf_url 的
// 3. 下载并验证 Content-Type 含 "pdf"

if !pdf_url: return Err(CliError::NotFound("No open access PDF available"))
response = request_with_retry(pdf_url, timeout=60s)
return DownloadResult { content: response.body, ... }
```

## read(identifier) -> ReadResult

```
// 1. 先尝试 API 的 fullText 字段 (如果 >500 字符直接用)
detail = GET /works/{identifier}
if detail["fullText"] && detail["fullText"].len() > 500:
    return ReadResult { full_text: detail["fullText"], sections: None, ... }

// 2. 降级到 download → pdf_oxide 提取文本
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
