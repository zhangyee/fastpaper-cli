# CrossRef

- **API类型**: REST JSON
- **基础URL**: `https://api.crossref.org`
- **认证**: 无需, 使用 `FASTPAPER_EMAIL` 进入 polite pool (fallback 硬编码项目邮箱)
- **能力**: search + get_by_doi (不支持 download / read)
- **特点**: DOI 注册机构, 支持精确 DOI 查询

## search(args: &SearchArgs) -> SearchResult

```
mailto = env::var("FASTPAPER_EMAIL").unwrap_or("yee.zhang@gmail.com")

params = {
    query: args.query,
    rows: min(args.limit, 1000),
    offset: args.offset,
    mailto: mailto,
}

// sort 映射
match args.sort {
    Relevance  => params["sort"] = "relevance",
    Date       => params["sort"] = "published",
    Citations  => params["sort"] = "is-referenced-by-count",
}
params["order"] = match args.order { Asc => "asc", Desc => "desc" }

// filter 映射 (CrossRef 用 filter 参数)
filters = []
if args.after:       filters.push("from-pub-date:{args.after}")
if args.before:      filters.push("until-pub-date:{args.before}")
if args.year:        filters.push("from-pub-date:{args.year},until-pub-date:{args.year}")
if args.open_access: filters.push("has-full-text:true")
if filters.len() > 0: params["filter"] = filters.join(",")

url = "{BASE_URL}/works"

// 指数退避重试 (最多3次)
response = request_with_retry(url, params, timeout=30s)

data = response.json()
items = data["message"]["items"]
total = data["message"]["total-results"]

for item in items:
    paper = parse_crossref_item(item)

    // 本地过滤
    if args.author && !paper.authors.any(|a| a.contains(args.author)): continue

    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.field:          warn("crossref: --field not supported, ignored")
if args.peer_reviewed:  warn("crossref: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### parse_crossref_item(item) -> Paper

```
doi = item["DOI"]
title = item["title"][0]  // title 是数组
authors = item["author"].map(|a| "{a.given} {a.family}".trim())
abstract_text = item["abstract"]

// 日期: 依次尝试 published → issued → created
published_date = extract_date(item, "published")
                || extract_date(item, "issued")
                || extract_date(item, "created")
//   extract_date: item[field]["date-parts"][0] → [year, month?, day?]
year = published_date.year

url = item["URL"] || "https://doi.org/{doi}"

// PDF URL:
//   检查 item.resource.primary.URL 是否以 .pdf 结尾
//   检查 item.link[] 中 content-type 含 "pdf" 的
pdf_url = ...

venue = item["container-title"][0]  // 期刊名
citations = item["is-referenced-by-count"]
fields = [item["type"]]

return Paper {
    id: doi,
    source: "crossref",
    open_access: None,  // CrossRef 不提供 OA 状态
    ...
}
```

## get_paper_by_doi(doi) -> Option<Paper>

```
// 精确 DOI 查询 (CrossRef 独有能力, 用于 `fastpaper get` 路由)
mailto = env::var("FASTPAPER_EMAIL").unwrap_or("yee.zhang@gmail.com")
url = "{BASE_URL}/works/{doi}"
params = { mailto: mailto }
response = request_with_retry(url, params, timeout=30s)
if response.status == 404: return None
item = response.json()["message"]
return Some(parse_crossref_item(item))
```

## download / read

```
return Err(CliError::NotSupported {
    source: "crossref",
    action: "download" / "read",
    hint: "CrossRef provides metadata only. Use the DOI with another source:\n  fastpaper download semantic <DOI>\n  fastpaper get <DOI> --resolve",
})
```
