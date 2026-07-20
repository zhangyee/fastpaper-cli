# PMC (PubMed Central)

- **API类型**: E-utilities REST (XML)
- **搜索URL**: `https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi`
- **详情URL**: `https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi`
- **认证**: 无需 (可选 `NCBI_API_KEY` 提升 rate limit: 3→10 req/s)
- **能力**: search + download + read

## search(args: &SearchArgs) -> SearchResult

```
// 第1步: 搜索获取 PMCID 列表
search_params = {
    db: "pmc",
    term: build_query(args),
    retmax: args.limit,
    retstart: args.offset,
    retmode: "xml",
    tool: "fastpaper",
    email: FASTPAPER_EMAIL || "yee.zhang@gmail.com",
}
if env::var("NCBI_API_KEY").is_ok():
    search_params["api_key"] = env::var("NCBI_API_KEY")

search_response = request_with_retry(EUTILS_SEARCH_URL, search_params, timeout=30s)
pmcids = parse_xml(search_response).find_all(".//Id").map(|e| e.text)
if pmcids.is_empty(): return SearchResult { results: [], ... }

// 第2步: 用 efetch 获取完整元数据 (包含摘要)
fetch_params = {
    db: "pmc",
    id: pmcids.join(","),
    retmode: "xml",
    tool: "fastpaper",
    email: FASTPAPER_EMAIL || "yee.zhang@gmail.com",
}
if env::var("NCBI_API_KEY").is_ok():
    fetch_params["api_key"] = env::var("NCBI_API_KEY")

fetch_response = request_with_retry(EUTILS_FETCH_URL, fetch_params, timeout=30s)
fetch_root = parse_xml(fetch_response)

// 第3步: 解析文章记录
for article in fetch_root.find_all(".//article"):
    front = article.find(".//front")
    pmcid = front.find(".//article-id[@pub-id-type='pmc']").text
    if !pmcid: continue
    pmcid = "PMC{pmcid}"

    title = front.find(".//article-title").all_text().trim()
    if !title: continue

    // 作者
    authors = []
    for contrib in front.find_all(".//contrib[@contrib-type='author']"):
        name = contrib.find("name")
        if name:
            given = name.find("given-names").text
            surname = name.find("surname").text
            authors.push("{given} {surname}".trim())

    // 摘要 (efetch 提供完整摘要)
    abstract_text = front.find(".//abstract").all_text().trim()

    // DOI
    doi = front.find(".//article-id[@pub-id-type='doi']").text

    // 日期
    pub_date = front.find(".//pub-date[@pub-type='epub']")
               || front.find(".//pub-date")
    year = pub_date.find("year").text
    journal = front.find(".//journal-title").text

    paper = Paper {
        id: pmcid,
        title: title,
        authors: authors,
        abstract_text: if abstract_text.is_empty() { None } else { Some(abstract_text) },
        year: year.parse().ok(),
        doi: if doi.is_empty() { None } else { Some(doi) },
        url: Some("https://www.ncbi.nlm.nih.gov/pmc/articles/{pmcid}/"),
        pdf_url: Some("https://www.ncbi.nlm.nih.gov/pmc/articles/{pmcid}/pdf/"),
        venue: if journal.is_empty() { None } else { Some(journal) },
        citations: None,
        fields: [],
        open_access: Some(true),
        source: "pmc",
    }
    papers.push(paper)

// 不支持的 filter 发 stderr 警告
if args.sort == Citations: warn("pmc: --sort citations not supported, ignored")
if args.field:             warn("pmc: --field not supported, ignored")
if args.peer_reviewed:     warn("pmc: --peer-reviewed not supported, ignored")

return SearchResult { results: papers, ... }
```

### build_query(args)

```
query = args.query
if args.author: query += " AND {args.author}[Author]"
if args.year:   query += " AND {args.year}[pdat]"
if args.after:  query += " AND {args.after}:3000[pdat]"
if args.before: query += " AND 1900:{args.before}[pdat]"
if args.open_access: query += " AND open access[filter]"
return query
```

## download(identifier) -> DownloadResult

```
if !identifier.starts_with("PMC"): identifier = "PMC{identifier}"
pdf_url = "https://www.ncbi.nlm.nih.gov/pmc/articles/{identifier}/pdf/"
response = request_with_retry(pdf_url, timeout=60s)

// 验证 Content-Type 含 "pdf"
if "pdf" not in response.content_type:
    return Err(CliError::Permission("Not an open access PDF"))

return DownloadResult { content: response.body, ... }
```

## read(identifier) -> ReadResult

PMC 优先使用 XML 全文获取结构化章节数据，PDF 提取作为 fallback。

```
if !identifier.starts_with("PMC"): identifier = "PMC{identifier}"

// 优先: efetch XML 全文 → 结构化 sections
fetch_params = {
    db: "pmc",
    id: identifier.strip_prefix("PMC"),
    rettype: "xml",
    tool: "fastpaper",
    email: FASTPAPER_EMAIL || "yee.zhang@gmail.com",
}
if env::var("NCBI_API_KEY").is_ok():
    fetch_params["api_key"] = env::var("NCBI_API_KEY")

xml_response = request_with_retry(EUTILS_FETCH_URL, fetch_params, timeout=30s)
root = parse_xml(xml_response)

// 解析元数据
front = root.find(".//front")
metadata = parse_front_metadata(front)  // 同 search 的解析逻辑

// 解析结构化章节
body = root.find(".//body")
sections = HashMap::new()

// 摘要
abstract_el = root.find(".//abstract")
if abstract_el: sections["abstract"] = abstract_el.all_text().trim()

// body 中的 <sec> 标签
for sec in body.find_all("sec"):
    sec_title = sec.find("title").text.to_lowercase()
    sec_text = sec.all_text().trim()
    // 映射 sec_title 到标准 section 名
    key = match sec_title {
        含 "introduction" => "introduction",
        含 "method"       => "methods",
        含 "result"       => "results",
        含 "discussion"   => "discussion",
        含 "conclusion"   => "conclusion",
        _                 => sec_title,
    }
    sections[key] = sec_text

// 参考文献
back = root.find(".//back")
ref_list = back.find(".//ref-list")
if ref_list: sections["references"] = ref_list.all_text().trim()

full_text = body.all_text().trim()

if full_text.len() < 100:
    // Fallback: 下载 PDF → pdf_oxide 提取文本
    pdf_bytes = download(identifier).content
    full_text = pdf_oxide::extract_text(pdf_bytes)
    sections = None  // PDF 无结构化数据，交给 CLI 层启发式切分

return ReadResult {
    metadata: metadata,
    full_text: full_text,
    sections: Some(sections),
    ...
}
```
