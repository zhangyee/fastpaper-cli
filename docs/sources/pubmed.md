# PubMed

- **API 类型**: E-utilities REST(两步:esearch → efetch)
- **基础 URL**: `https://eutils.ncbi.nlm.nih.gov`
- **认证**: 无需(可选 `NCBI_API_KEY`,rate limit 3→10 req/s)
- **能力**: search + get
- **实现**: `src/sources/pubmed.rs`(以代码为准)

## 搜索(两步)

1. **esearch** `GET /entrez/eutils/esearch.fcgi`,取 PMID 列表。
   - 参数:`db=pubmed`、`term={encode(query)}`、`retmax={max_results}`、`retmode=json`、`tool=fastpaper`;设置了 `FASTPAPER_EMAIL` 才带 `email`;有 key 则追加 `&api_key=`。
   - 解析 `esearchresult.idlist`;为空则返回空列表。
2. **efetch** `POST /entrez/eutils/efetch.fcgi`(表单),取详情,**每批 200 个 PMID**。
   - 表单字段:`db=pubmed`、`id={pmids join ","}`、`retmode=xml`、`tool`、`email`(+可选 `api_key`)。
   - 为什么 POST + 分批:旧实现把全部 PMID 拼进 GET URL,实测 `-n 500` 起 NCBI 回 414,`-n 10000` 连 URL 都发不出去(http 库报 uri too long)。官方建议超过约 200 个 UID 用 POST。每批 200 条实测 3.5 MB、5.5 s,留在单请求 10 MB 读取 / 30 s 上限之内;一次 1000 条是 17 MB、39 s。
   - 结果按 esearch 给的顺序重排(pubmed 实测会照提交顺序返回,重排是兜底)。
   - `PubmedBookArticle`(如 GeneReviews 书籍章节)不解析,会从结果里缺席——`-n 1000` 实测 999 条就是这个原因。
- `term` 接受 PubMed 原生检索语法(字段标签如 `[Author]`、`[pdat]`);CLI 把 query **原样传入**,不做本地字段拼接。
- `get_by_pmid(base, pmid)`:单条 efetch。

## 响应映射(PubmedArticle → Paper)

- `id`:`PMID`(首个)
- `title`:`ArticleTitle`
- `authors`:每个 `Author` 取 `LastName` + `Initials`,拼为 `"Last Init"`
- `abstract_text`:所有 `AbstractText` 文本以空格拼接(空则 `None`)
- `year`:`PubDate/Year`
- `doi`:`ELocationID[@EIdType="doi"]`
- `url`:`https://pubmed.ncbi.nlm.nih.gov/{pmid}/`
- `pdf_url` / `venue` / `citations` / `open_access`:均无;`fields`:空

## 注意

- HTTP 429、502、503、504 指数退避重试(共最多 3 次);其余 5xx 直接报错。
- `esearch` / `efetch` 的 HTTP 2xx 响应若缺少正常成功结构或无法解析也会重试;三次仍失败会注明阶段并报告截断后的响应摘要。合法空 `idlist` / `PubmedArticleSet` 仍按零结果处理。
- PubMed 仅元数据,无 PDF;正文需转 PMC(`download pmc <PMCID>`)。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `retmax`(≤1000:5 批 efetch,实测约 15 s。esearch 本身最多只能翻到前 10000 条,更多用 `--offset`) |
| `--offset` | `retstart` |
| `--author` | `term` 内 `{name}[au]` |
| `--year` | `term` 内 `{year}[dp]` |
| `--after` / `--before` | `datetype=pdat` + `mindate` / `maxdate`(`YYYY/MM/DD`) |
| `--sort` | `sort=relevance` / `pub_date`;**citations 不支持** |
| `--field` / `--open-access` | 不支持。MeSH 主题词不等同于学科领域,套在 `--field` 下会误导 |
