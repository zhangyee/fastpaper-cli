# OpenAIRE

- **API类型**: REST JSON
- **基础URL**: `https://api.openaire.eu`
- **搜索端点**: `/search/researchProducts`
- **认证**: 无需
- **能力**: search only (不支持 download / read)

## search(args: &SearchArgs) -> SearchResult

```
params = {
    keywords: args.query,
    size: min(args.limit, 50),
    page: (args.offset / min(args.limit, 50)) + 1,
    format: "json",
}

// 过滤参数映射
if args.year:        params["fromPublicationDate"] = "{args.year}-01-01"
                     params["toPublicationDate"] = "{args.year}-12-31"
if args.after:       params["fromPublicationDate"] = args.after
if args.before:      params["toPublicationDate"] = args.before
if args.open_access: params["OA"] = "true"

// 指数退避重试 (最多3次, 429/5xx)
response = request_with_retry("{BASE_URL}/search/researchProducts", params, timeout=30s)
data = response.json()
results = data["response"]["results"]["result"] || []

for result in results:
    paper = parse_openaire_result(result)
    if !paper: continue

    // 本地过滤
    if args.author && !paper.authors.any(|a| a.contains(args.author)): continue
    if args.field && !paper.fields.any(|f| f.contains(args.field)): continue

    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.sort != Relevance: warn("openaire: --sort not supported, ignored")
if args.peer_reviewed:     warn("openaire: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, ... }
```

### parse_openaire_result(result) -> Option<Paper>

```
metadata = result["metadata"]["oaf:entity"]["oaf:result"]
if !metadata: return None

// 标题
title = metadata["title"]
// 可能是数组或字符串; 如果数组, 优先取 classid="main title" 的
if title is array: title = title.find(|t| t["@classid"] == "main title")["$"] || title[0]["$"]
if !title: return None

// 作者
creators = metadata["creator"]
authors = if creators is array { creators.map(|c| c["$"]) } else { [creators["$"]] }

// 摘要
description = metadata["description"]
abstract_text = if description is array { description[0]["$"] } else { description["$"] }

// DOI: 从 pid 数组中提取
pids = metadata["pid"] || []
doi = pids.find(|p| p["@classid"] == "doi")["$"]
if !doi: doi = extract_doi(abstract_text)

// 日期
pub_date = metadata["dateofacceptance"]["$"]
year = pub_date[..4].parse().ok()

// URL: 从 children > instance > webresource > url 提取
instances = metadata["children"]["instance"] || []
url = None
pdf_url = None
for instance in instances:
    webresources = instance["webresource"] || []
    for wr in webresources:
        u = wr["url"]["$"]
        if u.starts_with("http"):
            if !url: url = Some(u)
            if u.ends_with(".pdf") || u.contains("/pdf"): pdf_url = Some(u)

paper_id = result["header"]["dri:objIdentifier"]["$"] || "openaire_{hash(title)}"

return Some(Paper {
    id: paper_id,
    source: "openaire",
    open_access: Some(metadata["bestaccessright"]["@classid"] == "OPEN"),
    venue: None,
    citations: None,
    fields: [],
    ...
})
```

## download / read

```
return Err(CliError::NotSupported {
    source: "openaire",
    action: "download" / "read",
    hint: "OpenAIRE provides metadata only. Use the pdf_url from search results if available.",
})
```
