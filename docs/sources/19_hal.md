# HAL

- **API类型**: REST JSON (Solr 接口)
- **搜索URL**: `https://api.archives-ouvertes.fr/search/`
- **认证**: 无需
- **能力**: search + download + read
- **特点**: 法国国家开放存档

## search(args: &SearchArgs) -> SearchResult

```
limit = clamp(args.limit, 1, 10000)

// 请求的字段列表 (最小化 payload):
fields = "halId_s,title_s,authFullName_s,abstract_s,doiId_s,
          publicationDateY_i,producedDateY_i,submittedDate_s,
          linkExtUrl_s,fileMain_s,uri_s,docType_s"

// 过滤参数映射 (Solr fq 语法)
fq_parts = []
if args.year:   fq_parts.push("publicationDateY_i:{args.year}")
if args.after:  fq_parts.push("publicationDateY_i:[{parse_year(args.after)} TO *]")
if args.before: fq_parts.push("publicationDateY_i:[* TO {parse_year(args.before)}]")
if args.field:  fq_parts.push("domain_s:{args.field}")   // "spi", "math", "sdv" 等
if args.author: fq_parts.push("authFullName_s:\"{args.author}\"")

// sort 映射
sort = match args.sort {
    Relevance  => "score desc",
    Date       => if args.order == Asc { "producedDateY_i asc" } else { "producedDateY_i desc" },
    Citations  => { warn("hal: --sort citations not supported, using relevance"); "score desc" },
}

params = {
    q: args.query,
    fl: fields,
    rows: limit,
    start: args.offset,
    wt: "json",
    sort: sort,
}
if fq_parts.len() > 0: params["fq"] = fq_parts.join(" AND ")

// 指数退避重试
response = request_with_retry(SEARCH_URL, params, timeout=20s)
data = response.json()
total = data["response"]["numFound"]

for doc in data["response"]["docs"]:
    paper = parse_doc(doc)
    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.open_access:    warn("hal: --open-access ignored (all HAL content is open access)")
if args.peer_reviewed:  warn("hal: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### parse_doc(doc) -> Paper

```
hal_id = doc["halId_s"]
title = doc["title_s"]       // 可能是数组, 取第一个
if title is array: title = title[0]
authors = doc["authFullName_s"]  // 数组
abstract_text = doc["abstract_s"]    // 数组 → join(" ")
if abstract_text is array: abstract_text = abstract_text.join(" ")
doi = doc["doiId_s"]            // 可能是数组, 取第一个
if doi is array: doi = doi[0]

year = doc["publicationDateY_i"] || doc["producedDateY_i"]
pdf_url = doc["fileMain_s"]
record_url = doc["uri_s"] || "https://hal.archives-ouvertes.fr/{hal_id}"

return Paper {
    id: "hal:{hal_id}",
    source: "hal",
    open_access: Some(true),
    venue: None,
    citations: None,
    fields: [],
    ...
}
```

## download(identifier) -> DownloadResult

```
hal_id = normalise_id(identifier)  // 去掉 "hal:" 前缀

// 解析 PDF URL
pdf_url = "https://hal.archives-ouvertes.fr/{hal_id}/document"

// HEAD 请求检查可用性
head_response = ureq::head(pdf_url).call()
if head_response.status() != 200:
    return Err(CliError::NotFound("No open-access PDF, may be metadata-only or embargoed"))
// 验证 content-type 含 "pdf"
if "pdf" not in head_response.content_type():
    return Err(CliError::NotFound("Resource is not a PDF"))

dl_response = request_with_retry(pdf_url, timeout=60s)
return DownloadResult { content: dl_response.body, ... }
```

## read(identifier) -> ReadResult

```
pdf_bytes = download(identifier).content
text = pdf_oxide::extract_text(pdf_bytes)
return ReadResult { full_text: text, sections: None, ... }
```
