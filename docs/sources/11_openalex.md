# OpenAlex

- **API类型**: REST JSON
- **基础URL**: `https://api.openalex.org/works`
- **认证**: 无需, 使用 `FASTPAPER_EMAIL` 进入 polite pool (fallback 硬编码项目邮箱)
- **能力**: search only (不支持 download / read)
- **特点**: 摘要以**倒排索引**格式存储, 需重建

## search(args: &SearchArgs) -> SearchResult

```
mailto = env::var("FASTPAPER_EMAIL").unwrap_or("yee.zhang@gmail.com")

params = {
    search: args.query,
    per_page: min(args.limit, 200),
    page: (args.offset / min(args.limit, 200)) + 1,
    mailto: mailto,
}

// sort 映射
match args.sort {
    Relevance  => {},                                    // 默认 relevance
    Date       => params["sort"] = "publication_date:desc",
    Citations  => params["sort"] = "cited_by_count:desc",
}
if args.order == Asc:
    // 反转 sort direction
    params["sort"] = params["sort"].replace(":desc", ":asc")

// filter 映射 (OpenAlex 用 filter 参数)
filters = []
if args.year:        filters.push("publication_year:{args.year}")
if args.after:       filters.push("from-publication-date:{args.after}")
if args.before:      filters.push("to-publication-date:{args.before}")
if args.open_access: filters.push("is_oa:true")
if args.author:      filters.push("authorships.author.display_name.search:{args.author}")
if args.field:       filters.push("concepts.display_name.search:{args.field}")
if filters.len() > 0: params["filter"] = filters.join(",")

// 指数退避重试
response = request_with_retry(BASE_URL, params, timeout=30s)
data = response.json()
results = data["results"]
total = data["meta"]["count"]

for item in results:
    paper_id = item["id"].replace("https://openalex.org/", "")  // "W2741809807"
    title = item["title"]
    if !title: continue

    authors = item["authorships"].map(|a| a["author"]["display_name"])

    // 重建摘要 (倒排索引 → 文本)
    abstract_text = reconstruct_abstract(item["abstract_inverted_index"])

    doi = item["doi"].replace("https://doi.org/", "")
    if !doi: doi = extract_doi(abstract_text)

    // URL & PDF:
    primary_location = item["primary_location"]
    url = primary_location["landing_page_url"] || item["id"]
    pdf_url = primary_location["pdf_url"]
    if !pdf_url && item["open_access"]["is_oa"]:
        pdf_url = item["open_access"]["oa_url"]

    year = item["publication_year"]
    concepts = item["concepts"].map(|c| c["display_name"])[..5]
    citations = item["cited_by_count"]

    papers.push(Paper {
        id: paper_id,
        source: "openalex",
        fields: concepts,
        open_access: Some(item["open_access"]["is_oa"]),
        venue: primary_location["source"]["display_name"],
        ...
    })

// 不支持的 filter 发 stderr 警告
if args.peer_reviewed: warn("openalex: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, total: Some(total), ... }
```

### reconstruct_abstract(inverted_index) -> Option<String>

```
if !inverted_index: return None
word_positions = []
for (word, positions) in inverted_index:
    for pos in positions:
        word_positions.push((pos, word))
word_positions.sort_by_key(|(pos, _)| pos)
Some(word_positions.map(|(_, word)| word).join(" "))
```

## download / read

```
return Err(CliError::NotSupported {
    source: "openalex",
    action: "download" / "read",
    hint: "OpenAlex provides metadata only. Use the pdf_url from search results if available.",
})
```
