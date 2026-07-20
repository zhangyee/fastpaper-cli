# PubMed

- **API类型**: E-utilities REST (XML)
- **搜索URL**: `https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi`
- **详情URL**: `https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi`
- **认证**: 无需 (可选 `NCBI_API_KEY` 提升 rate limit: 3→10 req/s)
- **能力**: search only (不支持 download / read)

## search(args: &SearchArgs) -> SearchResult

```
// 第1步: 搜索获取 PMID 列表
search_params = {
    db: "pubmed",
    term: build_query(args),
    retmax: args.limit,
    retstart: args.offset,
    retmode: "xml",
    tool: "fastpaper",
    email: FASTPAPER_EMAIL || "yee.zhang@gmail.com",
}
if env::var("NCBI_API_KEY").is_ok():
    search_params["api_key"] = env::var("NCBI_API_KEY")

// sort 映射
match args.sort {
    Relevance => {},                        // PubMed 默认 relevance
    Date      => search_params["sort"] = "pub+date",
    Citations => warn("pubmed: --sort citations not supported, using relevance"),
}

// 指数退避重试
search_response = request_with_retry(SEARCH_URL, search_params, timeout=30s)
search_root = parse_xml(search_response.body)
ids = search_root.find_all(".//Id").map(|id| id.text)
if ids.is_empty(): return SearchResult { results: [], ... }

// 第2步: 用 PMID 列表批量获取详情
fetch_params = {
    db: "pubmed",
    id: ids.join(","),
    retmode: "xml",
    tool: "fastpaper",
    email: FASTPAPER_EMAIL || "yee.zhang@gmail.com",
}
if env::var("NCBI_API_KEY").is_ok():
    fetch_params["api_key"] = env::var("NCBI_API_KEY")

fetch_response = request_with_retry(FETCH_URL, fetch_params, timeout=30s)
fetch_root = parse_xml(fetch_response.body)

for article in fetch_root.find_all(".//PubmedArticle"):
    pmid = article.find(".//PMID").text.trim()
    title = article.find(".//ArticleTitle").all_text().trim()
    if !pmid || !title: continue

    authors = []
    for author in article.find_all(".//Author"):
        last = author.find("LastName").text
        init = author.find("Initials").text
        authors.push("{last} {init}")

    abstract_text = article.find_all(".//AbstractText")
                    .map(|e| e.all_text().trim()).join(" ")

    year = article.find(".//PubDate/Year").text
    doi = article.find(".//ELocationID[@EIdType='doi']").text
    if !doi && abstract_text: doi = extract_doi(abstract_text)

    paper = Paper {
        id: pmid,
        title: title,
        authors: authors,
        abstract_text: if abstract_text.is_empty() { None } else { Some(abstract_text) },
        year: year.parse().ok(),
        doi: if doi.is_empty() { None } else { Some(doi) },
        url: Some("https://pubmed.ncbi.nlm.nih.gov/{pmid}/"),
        pdf_url: None,             // PubMed 无直接 PDF
        venue: None,
        citations: None,
        fields: [],
        open_access: None,
        source: "pubmed",
    }

    // 本地日期过滤
    if args.after  && paper.year < parse_year(args.after):  continue
    if args.before && paper.year > parse_year(args.before): continue

    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.field:          warn("pubmed: --field not supported, ignored")
if args.open_access:    warn("pubmed: --open-access not supported, ignored")
if args.peer_reviewed:  warn("pubmed: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, ... }
```

### build_query(args)

```
// 将 CLI 过滤参数映射为 PubMed 查询语法
query = args.query
if args.author: query += " AND {args.author}[Author]"
if args.year:   query += " AND {args.year}[pdat]"
if args.after:  query += " AND {args.after}:3000[pdat]"
if args.before: query += " AND 1900:{args.before}[pdat]"
return query
```

## download / read

```
// 均不支持
return Err(CliError::NotSupported {
    source: "pubmed",
    action: "download" / "read",
    hint: "PubMed only provides metadata. Try:\n  fastpaper download pmc <PMCID>\n  fastpaper get <PMID> --resolve",
})
```
