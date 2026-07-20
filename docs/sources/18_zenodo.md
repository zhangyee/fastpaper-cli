# Zenodo

- **API类型**: REST JSON
- **基础URL**: `https://zenodo.org/api`
- **认证**: 无需
- **能力**: search + download + read
- **特点**: CERN 运营, 通用型存储库, 支持论文/数据集/软件

## search(args: &SearchArgs) -> SearchResult

```
limit = clamp(args.limit, 1, 200)

params = {
    q: args.query,
    size: limit,
    page: (args.offset / limit) + 1,
    type: "publication",
}

// sort 映射
match args.sort {
    Relevance  => {},                             // 默认
    Date       => params["sort"] = "mostrecent",
    Citations  => warn("zenodo: --sort citations not supported, ignored"),
}

// 过滤参数映射 (Zenodo 用 Elasticsearch query syntax)
if args.year:
    params["q"] = "{args.query} AND publication_date:[{args.year}-01-01 TO {args.year}-12-31]"
if args.after:
    params["q"] = "{args.query} AND publication_date:[{args.after} TO *]"
if args.before:
    params["q"] = "{args.query} AND publication_date:[* TO {args.before}]"
if args.open_access:
    params["access_right"] = "open"

// 指数退避重试
response = request_with_retry("{BASE_URL}/records", params, timeout=20s)
data = response.json()
total = data["hits"]["total"]

for hit in data["hits"]["hits"]:
    paper = parse_record(hit)

    // 本地过滤
    if args.author && !paper.authors.any(|a| a.contains(args.author)): continue
    if args.field && !paper.fields.any(|f| f.contains(args.field)): continue

    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.peer_reviewed: warn("zenodo: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### parse_record(hit) -> Paper

```
meta = hit["metadata"]
record_id = hit["id"]
doi = hit["doi"] || meta["doi"]
title = meta["title"]
authors = meta["creators"].map(|c| c["name"] || "{c.given_name} {c.family_name}")
abstract_text = meta["description"]  // 可能含 HTML 标签, 需 strip_tags

pub_date = meta["publication_date"][..10]  // "YYYY-MM-DD"
year = pub_date[..4].parse().ok()

// PDF URL: hit["files"][] 中 key 以 .pdf 结尾的, 取 links.self 或 links.download
pdf_url = hit["files"].find(|f| f["key"].ends_with(".pdf"))
          .map(|f| f["links"]["self"] || f["links"]["download"])

record_url = hit["links"]["html"] || "https://zenodo.org/record/{record_id}"

fields = meta["keywords"] || []

return Paper {
    id: "zenodo:{record_id}",
    source: "zenodo",
    open_access: Some(meta["access_right"] == "open"),
    venue: None,
    citations: None,
    ...
}
```

## download(identifier) -> DownloadResult

```
record_id = extract_record_id(identifier)
//   "zenodo:" 前缀 → 去掉
//   DOI 格式 "10.5281/zenodo.1234567" → 提取数字
//   纯数字 → 直接用

response = request_with_retry("{BASE_URL}/records/{record_id}", timeout=20s)
record = response.json()

pdf_file = record["files"].find(|f| f["key"].ends_with(".pdf"))
if !pdf_file: return Err(CliError::NotFound("No open-access PDF in this Zenodo record"))

pdf_url = pdf_file["links"]["self"] || pdf_file["links"]["download"]
dl_response = request_with_retry(pdf_url, timeout=60s)
return DownloadResult { content: dl_response.body, ... }
```

## read(identifier) -> ReadResult

```
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
