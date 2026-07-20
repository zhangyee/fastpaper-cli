# Semantic Scholar

- **API类型**: REST JSON (Graph API v1)
- **基础URL**: `https://api.semanticscholar.org/graph/v1`
- **认证**: 可选 `SEMANTIC_SCHOLAR_API_KEY` (提升 rate limit: 1→100 req/s)
- **能力**: search + download + read
- **特点**: 支持多种 paper_id 格式 (S2 ID, DOI:, ARXIV:, PMID:, URL: 等)

## search(args: &SearchArgs) -> SearchResult

```
fields = ["title", "abstract", "year", "citationCount", "authors",
          "url", "publicationDate", "externalIds", "fieldsOfStudy",
          "openAccessPdf", "venue"]

params = {
    query: args.query,
    offset: args.offset,
    limit: args.limit,
    fields: fields.join(","),
}

// 过滤参数映射
if args.year: params["year"] = "{args.year}"
// S2 支持年份范围: "2019", "2016-2020", "2010-", "-2015"
if args.after && !args.year:
    params["year"] = "{parse_year(args.after)}-"
if args.before && !args.year:
    params["year"] = "-{parse_year(args.before)}"
if args.field:
    params["fieldsOfStudy"] = args.field

// sort 映射: S2 搜索 API 暂不支持自定义排序
if args.sort != Relevance: warn("semantic: --sort only supports relevance for search, ignored")

// 请求 (含 API key + 退避重试)
response = request_api("paper/search", params)
//   request_api 逻辑:
//     如有 SEMANTIC_SCHOLAR_API_KEY, 加 header x-api-key
//     指数退避重试 (最多3次)
//     403 + 有 key → 去掉 key 重试一次
//     429 → 读 Retry-After header, 指数退避等待

data = response.json()
results = data["data"]

for item in results:
    authors = item["authors"].map(|a| a["name"])

    // pdf_url 提取
    pdf_url = item["openAccessPdf"]["url"]
    // arXiv abs URL → 转为 pdf URL
    if pdf_url && pdf_url.contains("arxiv.org/abs/"):
        pdf_url = pdf_url.replace("/abs/", "/pdf/")

    doi = item["externalIds"]["DOI"]
    if !doi: doi = extract_doi(item["abstract"])

    paper = Paper {
        id: item["paperId"],
        title: item["title"],
        authors: authors,
        abstract_text: item["abstract"],
        year: item["year"],
        doi: doi,
        url: item["url"],
        pdf_url: pdf_url,
        venue: item["venue"],
        citations: item["citationCount"],
        fields: item["fieldsOfStudy"] || [],
        open_access: Some(pdf_url.is_some()),
        source: "semantic",
    }

    // 本地过滤
    if args.author && !authors.any(|a| a.contains(args.author)): continue
    if args.open_access && pdf_url.is_none(): continue

    papers.push(paper)

if args.peer_reviewed: warn("semantic: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, ... }
```

## download(identifier) -> DownloadResult

```
// 获取单篇详情找 PDF URL
paper = get_paper_details(identifier)
//   GET "paper/{identifier}", fields=同 search
//   identifier 支持: S2 hash, DOI:xxx, ARXIV:xxx, PMID:xxx, URL:xxx

if !paper || !paper.pdf_url:
    return Err(CliError::NotFound("No open access PDF available"))

response = request_with_retry(paper.pdf_url, timeout=30s)
return DownloadResult { content: response.body, metadata: paper, ... }
```

## read(identifier) -> ReadResult

```
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
