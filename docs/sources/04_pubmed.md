# PubMed

- **API 类型**: E-utilities REST(两步:esearch → efetch)
- **基础 URL**: `https://eutils.ncbi.nlm.nih.gov`
- **认证**: 无需(可选 `NCBI_API_KEY`,rate limit 3→10 req/s)
- **能力**: search only(不支持 download / read)
- **实现**: `src/sources/pubmed.rs`(以代码为准)

## 搜索(两步)

1. **esearch** `GET /entrez/eutils/esearch.fcgi`,取 PMID 列表。
   - 参数:`db=pubmed`、`term={encode(query)}`、`retmax={max_results}`、`retmode=json`、`tool=fastpaper`、`email=yee.zhang@gmail.com`;有 key 则追加 `&api_key=`。
   - 解析 `esearchresult.idlist`;为空则返回空列表。
2. **efetch** `GET /entrez/eutils/efetch.fcgi`,取详情。
   - 参数:`db=pubmed`、`id={pmids join ","}`、`retmode=xml`、`tool`、`email`(+可选 `api_key`)。
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

- HTTP 429 退避重试(最多 3 次);5xx 直接报错。
- PubMed 仅元数据,无 PDF;正文需转 PMC(`download pmc <PMCID>`)。
