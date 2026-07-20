# ArXiv

- **API类型**: REST (Atom/XML feed)
- **基础URL**: `http://export.arxiv.org/api/query`
- **认证**: 无需
- **能力**: search + download + read

## search(args: &SearchArgs) -> SearchResult

```
// 构建查询字符串，映射 CLI 过滤参数到 arXiv query syntax
query_parts = ["all:{args.query}"]
if args.author:  query_parts.push("au:{args.author}")
if args.field:   query_parts.push("cat:{args.field}")

search_query = query_parts.join("+AND+")

// arXiv sortBy 映射
sort_by = match args.sort {
    Relevance  => "relevance",
    Date       => "submittedDate",
    Citations  => "relevance",   // arXiv 不支持按引用排序，warn + fallback
}
sort_order = match args.order {
    Asc  => "ascending",
    Desc => "descending",
}

params = {
    search_query: search_query,
    start: args.offset,
    max_results: args.limit,
    sortBy: sort_by,
    sortOrder: sort_order,
}

// 指数退避重试 (最多3次, 遇到 429/5xx)
response = request_with_retry(BASE_URL, params, timeout=30s)
if response.status != 200: return Err(...)

feed = parse_atom_xml(response.body)

for entry in feed.entries:
    // 日期过滤 (arXiv API 不直接支持日期范围，需本地过滤)
    pub_date = parse(entry.published, "%Y-%m-%dT%H:%M:%SZ")
    if args.after  && pub_date < parse(args.after):  continue
    if args.before && pub_date > parse(args.before): continue
    if args.year   && pub_date.year != args.year:    continue

    paper = Paper {
        id: entry.id 按 "/" 分割取最后一段,  // 如 "2301.12345"
        title: entry.title,
        authors: [author.name for author in entry.authors],
        abstract_text: Some(entry.summary),
        url: Some(entry.id),                   // 如 "http://arxiv.org/abs/2301.12345"
        pdf_url: entry.links 中 type="application/pdf" 的 href,
        year: Some(pub_date.year),
        doi: entry.doi || extract_doi(entry.summary) || extract_doi(entry.id)
             || entry.links 中 title="doi" 的 link.href 提取,
        venue: Some("arXiv preprint"),
        citations: None,                       // arXiv 不提供引用数
        fields: [tag.term for tag in entry.tags],
        open_access: Some(true),               // arXiv 全部开放
        source: "arxiv",
    }
    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.open_access:    warn("arxiv: --open-access ignored (all arXiv papers are open access)")
if args.peer_reviewed:  warn("arxiv: --peer-reviewed not supported, ignored")

return SearchResult { source: "arxiv", query: args.query, results: papers, ... }
```

## download(identifier) -> DownloadResult

```
pdf_url = "https://arxiv.org/pdf/{identifier}.pdf"
response = request_with_retry(pdf_url, timeout=30s)
// 返回字节内容，文件命名交给 CLI 层
return DownloadResult { content: response.body, ... }

// --source-files 支持 (arXiv 独有):
// source_url = "https://arxiv.org/e-print/{identifier}"
// 返回 tar.gz 内容
```

## read(identifier) -> ReadResult

```
// 下载 PDF → pdf_oxide 提取文本
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)

// arXiv 无结构化章节 API，sections 由 CLI 层启发式切分
return ReadResult {
    full_text: text,
    sections: None,  // CLI 层 fallback 到正则标题匹配切分
    ...
}
```
