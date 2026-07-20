# Europe PMC

- **API类型**: REST JSON
- **基础URL**: `https://www.ebi.ac.uk/europepmc/webservices/rest`
- **认证**: 无需
- **能力**: search + download + read

## search(args: &SearchArgs) -> SearchResult

```
params = {
    query: build_query(args),
    pageSize: min(args.limit, 100),
    format: "json",
    resultType: "core",
    cursorMark: "*",               // 分页游标
}

// sort 映射
match args.sort {
    Relevance => {},                // 默认 relevance
    Date      => params["sort"] = "DATE_DESC" / "DATE_ASC",
    Citations => params["sort"] = "CITED_DESC" / "CITED_ASC",
}

// offset 映射: Europe PMC 使用 cursorMark 分页，需跳过 offset 条
// 如 offset > 0: 先请求 offset 条丢弃

// 指数退避重试
response = request_with_retry("{BASE_URL}/search", params, timeout=30s)
data = response.json()
results = data["resultList"]["result"]

for item in results:
    paper = parse_item(item)
    papers.push(paper)

return SearchResult { results: papers, ... }
```

### build_query(args)

```
query = args.query
if args.author:      query += " AND AUTH:\"{args.author}\""
if args.year:        query += " AND PUB_YEAR:{args.year}"
if args.after:       query += " AND FIRST_PDATE:[{args.after} TO *]"
if args.before:      query += " AND FIRST_PDATE:[* TO {args.before}]"
if args.open_access: query += " AND OPEN_ACCESS:y"
return query
```

### parse_item(item) -> Paper

```
paper_id = item["id"]
// 按 source 类型加前缀: MED→"PMID:{id}", PMC→"PMC{id}", 其他保持原样

title = item["title"].trim()
authors = item["authorList"]["author"].map(|a| a["fullName"])
abstract_text = item["abstractText"]
doi = item["doi"] || item["doiId"] || extract_doi(abstract_text)

// 日期: 组合 pubYear + pubMonth + pubDay
year = item["pubYear"].parse().ok()

// URL: 从 fullTextUrlList.fullTextUrl 数组中
//   documentStyle="html" → landing_url
//   documentStyle="pdf"  → pdf_url
// 无 landing_url 则用 doi.org 或 pubmed/pmc URL

keywords = item["keywordList"]["keyword"]
is_open_access = item["isOpenAccess"] == "Y"

return Paper {
    id: paper_id,
    source: "europepmc",
    fields: keywords,
    open_access: Some(is_open_access),
    venue: item["journalTitle"],
    citations: item["citedByCount"].parse().ok(),
    ...
}
```

## download(identifier) -> DownloadResult

```
// 先获取论文详情找 PDF URL
details = get_paper_details(identifier)
//   构造查询: PMID:→"ext_id:X src:med", PMC→"ext_id:X src:pmc", DOI:→"doi:X"
//   GET /search, 取第一条结果

pdf_url = details.fullTextUrlList 中 documentStyle="pdf" 的 URL
if !pdf_url && details.pmcid:
    pdf_url = "https://ncbi.nlm.nih.gov/pmc/articles/{pmcid}/pdf/"
if !pdf_url: return Err(CliError::NotFound("No PDF available"))

response = request_with_retry(pdf_url, timeout=60s)
// 验证 Content-Type 含 "pdf"
return DownloadResult { content: response.body, ... }
```

## read(identifier) -> ReadResult

```
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
