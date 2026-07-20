# DOAJ

- **API类型**: REST JSON
- **基础URL**: `https://doaj.org/api`
- **认证**: 无需
- **能力**: search + download + read
- **特点**: 全部为开放获取内容, 支持 Lucene 查询语法

## search(args: &SearchArgs) -> SearchResult

```
limit = clamp(args.limit, 1, 100)

// 构建 Lucene 查询
query = build_lucene_query(args)

encoded_query = url_encode(query)
search_url = "{BASE_URL}/search/articles/{encoded_query}"
params = {
    page: (args.offset / limit) + 1,
    pageSize: limit,
}

// sort 映射
match args.sort {
    Relevance  => {},
    Date       => params["sort"] = "created_date:desc",
    Citations  => warn("doaj: --sort citations not supported, ignored"),
}

// 指数退避重试
response = request_with_retry(search_url, params, timeout=30s)
data = response.json()
results = data["results"]
total = data["total"]

for item in results:
    paper = parse_doaj_item(item)
    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.peer_reviewed: warn("doaj: --peer-reviewed not supported (all DOAJ journals are peer-reviewed), ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### build_lucene_query(args)

```
query = "({args.query})"
if args.year:   query += " AND bibjson.year:{args.year}"
if args.after:  query += " AND bibjson.year:[{parse_year(args.after)} TO *]"
if args.before: query += " AND bibjson.year:[* TO {parse_year(args.before)}]"
if args.author: query += " AND bibjson.author.name:\"{args.author}\""
if args.field:  query += " AND bibjson.subject.term:\"{args.field}\""
// --open-access 忽略 (DOAJ 全部 OA)
if args.open_access: warn("doaj: --open-access ignored (all DOAJ content is open access)")
return query
```

### parse_doaj_item(item) -> Paper

```
bibjson = item["bibjson"]
title = bibjson["title"]
authors = bibjson["author"].map(|a| a["name"])
abstract_text = bibjson["abstract"]  // 可能是 string 或 dict, 需处理

// DOI: bibjson.identifier[] 中 type="doi" 的 id
doi = bibjson["identifier"].find(|i| i["type"]=="doi")["id"]

// 日期: bibjson 的 year/month/day 组合
year = bibjson["year"].parse().ok()

// 期刊
venue = bibjson["journal"]["title"]

// PDF URL: bibjson.link[] 中 type="fulltext" 且 url 以 .pdf 结尾
pdf_url = bibjson["link"].find(|l| l["type"]=="fulltext" && l["url"].ends_with(".pdf"))["url"]

url = bibjson["link"].find(|l| l["type"]=="fulltext")["url"]
      || "https://doaj.org/article/{item['id']}"

fields = bibjson["subject"].map(|s| s["term"]) || []

return Paper {
    id: item["id"],
    source: "doaj",
    open_access: Some(true),
    venue: venue,
    citations: None,
    ...
}
```

## download(identifier) -> DownloadResult

```
papers = search(SearchArgs { query: identifier, limit: 1, ... })
if papers.is_empty(): return Err(CliError::NotFound("Paper not found in DOAJ"))
paper = papers[0]

pdf_url = paper.pdf_url
if !pdf_url && paper.doi:
    pdf_url = Some("https://doi.org/{doi}")  // 尝试
if !pdf_url: return Err(CliError::NotFound("No PDF available"))

response = request_with_retry(pdf_url, timeout=30s)
return DownloadResult { content: response.body, ... }
```

## read(identifier) -> ReadResult

```
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
