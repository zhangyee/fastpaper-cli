# PMC (PubMed Central)

- **API 类型**: E-utilities REST(两步:esearch → efetch)
- **基础 URL**: `https://eutils.ncbi.nlm.nih.gov`(下载 `https://www.ncbi.nlm.nih.gov`)
- **认证**: 无需(可选 `NCBI_API_KEY`,rate limit 3→10 req/s)
- **能力**: search + get + download
- **PDF 下载走 PMC Cloud Service（AWS Open Data）**:`https://pmc-oa-opendata.s3.amazonaws.com`
  - 文章页的 `/pdf/` 直链**不可编程使用**——对所有客户端(含浏览器 UA)一律返回 1817 字节的 "Preparing to download" HTML 拦截页
  - OA Web Service(`oa.fcgi`)给的是 `ftp://` 链接,而 NCBI 已停匿名 FTP(550);其 HTTPS 路径于 2026-04 迁入 `/deprecated/`,官方公告 **2026-08 删除**
  - 取法:先 `GET /?list-type=2&prefix=PMC{id}` 拿到带版本号的 key,再取 `{key}`(形如 `PMC5334499.1/PMC5334499.1.pdf`)
- **实现**: `src/sources/pmc.rs`(以代码为准)

## 搜索(两步)

1. **esearch** `GET /entrez/eutils/esearch.fcgi`,参数 `db=pmc`、`term={encode(query)}`、`retmax`、`retmode=json`、`tool=fastpaper`、`email=yee.zhang@gmail.com`(+可选 `api_key`);解析 `esearchresult.idlist`。
2. **efetch** `GET /entrez/eutils/efetch.fcgi`,参数 `db=pmc`、`id={ids}`、`rettype=xml`(注意是 `rettype` 而非 `retmode`)、`tool`、`email`(+可选 `api_key`)。
- query 原样传入 `term`,不做本地字段拼接。
- `get_by_pmc_id(base, id)`:先 strip `PMC` 前缀取数字 id,再单条 efetch。

## 响应映射(JATS `<article>/<front>` → Paper)

- `id` / `pmcid`:`article-id[@pub-id-type="pmcid"]`
- `doi`:`article-id[@pub-id-type="doi"]`
- `title`:`article-title`
- `authors`:`contrib[@contrib-type="author"]` 的 `given-names` + `surname`
- `abstract_text`:`<abstract>` 内文本拼接
- `year`:首个 `<year>`;`venue`:`journal-title`
- `url`:`https://www.ncbi.nlm.nih.gov/pmc/articles/{pmcid}/`
- `pdf_url`:同上 + `/pdf/`
- `open_access`:恒 `true`;`citations`:无;`fields`:空

## 下载 / read

- PDF:`{dl_base}/pmc/articles/PMC{numeric}/pdf/`(自动补 `PMC` 前缀)。
- read:下载 PDF 后本地提取文本,**无结构化章节解析**。

## 注意

- HTTP 429 退避重试(最多 3 次);5xx 直接报错。
- 仅开放获取文章有可下载 PDF。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` / `--offset` | `retmax`(≤10000) / `retstart` |
| `--author` | `term` 内 `{name}[au]` |
| `--year` | `term` 内 `{year}[dp]` |
| `--after` / `--before` | `datetype=pdat` + `mindate` / `maxdate` |
| `--open-access` | `term` 内 `open access[filter]`(PubMed 无此能力,PMC 有) |
| `--sort` | `sort=relevance` / `pub_date`;**citations 不支持** |
| `--field` | 不支持 |
