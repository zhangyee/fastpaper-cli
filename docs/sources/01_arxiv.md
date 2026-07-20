# arXiv

- **API 类型**: REST(Atom/XML feed)
- **基础 URL**: `http://export.arxiv.org/api/query`
- **认证**: 无需
- **能力**: search + download + read
- **实现**: `src/sources/arxiv.rs`(以代码为准)

## 搜索

GET 参数:`search_query`、`start`(偏移)、`max_results`、`sortBy`、`sortOrder`。

- 查询语法:`all:{query}`;作者 `au:{name}`;分类 `cat:{cat}`;多条件用 `+AND+` 连接。
- `sortBy` ∈ `relevance` | `submittedDate`;`sortOrder` ∈ `ascending` | `descending`。不支持按引用数排序。
- API 不支持日期范围过滤;需要时在本地按 entry 的 `published` 过滤。

## 响应映射(Atom entry → Paper)

- `id`:entry.id 按 `/` 分割取最后一段(如 `2301.12345`);entry.id 本身即落地页 URL
- `pdf_url`:links 中 `type="application/pdf"` 的 href
- `fields`:tags 的 term(如 `cs.CL`)
- `citations`:无(arXiv 不提供引用数)
- `open_access`:恒为 `true`
- `venue`:固定 `"arXiv preprint"`
- `doi`:部分 entry 有 DOI 扩展字段;缺失时可从 summary/链接提取

## 下载

- PDF:`https://arxiv.org/pdf/{id}.pdf`
- LaTeX 源码(`--source-files`,arXiv 独有):`https://arxiv.org/e-print/{id}`,tar.gz

## 注意

- 建议请求间隔约 3s。
- 无结构化章节 API,全文靠 PDF 提取。
