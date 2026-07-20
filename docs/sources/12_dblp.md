# DBLP

- **API类型**: REST XML (主) + HTML 爬取 (降级)
- **API URL**: `https://dblp.org/search/publ/api`
- **HTML URL**: `https://dblp.org/search/publ`
- **认证**: 无需
- **能力**: search only (不支持 download / read)
- **特点**: CS 专属; 无摘要; 自带 HTML 降级

## search(args: &SearchArgs) -> SearchResult

```
// 构建查询，映射过滤参数到 DBLP query syntax
query = args.query
if args.year:   query += " year:{args.year}"
// DBLP 支持年份范围: "year:2020:2023"
if args.after && !args.year:  query += " year:{parse_year(args.after)}:"
if args.before && !args.year: query += " year::{parse_year(args.before)}"
if args.author: query += " author:{args.author}"
if args.field:  query += " venue:{args.field}"   // DBLP 的 field 映射为 venue

params = {
    q: query,
    format: "xml",
    h: min(args.limit, 1000),
    f: args.offset,
}

// 指数退避重试 (最多3次, 429/5xx)
response = request_with_retry(API_URL, params, timeout=30s)

root = parse_xml(response)
hits = root.find_all(".//hit")

for hit in hits:
    paper = parse_dblp_hit(hit)
    papers.push(paper)

// 如果 API 返回0条可解析结果, 降级到 HTML 爬取
if papers.is_empty():
    return search_html_fallback(args)

// 不支持的 filter 发 stderr 警告
if args.sort != Relevance: warn("dblp: --sort not supported, ignored")
if args.open_access:       warn("dblp: --open-access not supported, ignored")
if args.peer_reviewed:     warn("dblp: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, ... }
```

### parse_dblp_hit(hit) -> Paper

```
info = hit.find("info")
title = info.find("title").text
authors = info.find_all("authors/author").map(|a| a.text)
venue = info.find("venue").text
year = info.find("year").text
url = info.find("url").text

// DOI: 从 <ee> 元素中提取
doi = None
for ee in info.find_all("ee"):
    if "doi.org" in ee.text: doi = Some(ee.text.split("doi.org/").last())
    elif ee.text.starts_with("10."): doi = Some(ee.text)

paper_id = url 按 "/" 分割取最后一段, 或 "dblp_{hash(title)}"

return Paper {
    id: paper_id,
    title: title,
    authors: authors,
    abstract_text: None,   // DBLP 无摘要
    year: year.parse().ok(),
    doi: doi,
    url: Some(url),
    pdf_url: None,         // DBLP 无直接 PDF
    venue: Some(venue),
    citations: None,
    fields: [],
    open_access: None,
    source: "dblp",
}
```

### search_html_fallback(args) -> SearchResult

```
// 降级 HTML 搜索 (scraper crate)
response = request_with_retry(HTML_URL, {q: args.query}, timeout=30s)
doc = scraper::Html::parse_document(response.body)
entries = doc.select(".publ-list .entry")

for entry in entries:
    title = entry.select_one(".title").text()
    doi = extract_doi(entry.select("li.ee a[href]").attr("href"))
    year = entry.select_one(".year").text().parse().ok()
    authors = entry.select("[itemprop='author'] [itemprop='name']").map(|e| e.text())
    papers.push(Paper { source: "dblp", ... })

return SearchResult { results: papers[..args.limit], ... }
```

## download / read

```
return Err(CliError::NotSupported {
    source: "dblp",
    action: "download" / "read",
    hint: "DBLP provides metadata only. Use the DOI with another source.",
})
```
