# arXiv

- **API 类型**: REST(Atom/XML feed)
- **基础 URL**: `http://export.arxiv.org/api/query`
- **认证**: 无需
- **能力**: search + get + download
- **实现**: `src/sources/arxiv.rs`(以代码为准)

## 搜索

GET 参数:`search_query`、`start`(偏移)、`max_results`、`sortBy`、`sortOrder`。

- 查询语法:`all:{query}`;作者 `au:{name}`;分类 `cat:{cat}`;多条件用 `+AND+` 连接。
- `sortBy` ∈ `relevance` | `submittedDate`;`sortOrder` ∈ `ascending` | `descending`。不支持按引用数排序。
- 日期范围**由 API 原生支持**:`search_query` 内 `submittedDate:[YYYYMMDDHHMM TO YYYYMMDDHHMM]`,与其他条件用 `+AND+` 连接。

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
- LaTeX 源码:`https://arxiv.org/e-print/{id}`(单文件投稿直接给原文件)或 `https://arxiv.org/src/{id}`(恒为 tar.gz)。**CLI 目前不暴露此能力**——`--source-files` 从未实现,已移除。

## 注意

- 限速([API 使用条款](https://info.arxiv.org/help/api/tou.html)):每 3 秒最多一个请求、同一时间只开一个连接,按调用方控制的所有机器合计。
  - 每次 fastpaper 调用是独立进程,进程内节流管不到并行调用,所以 `search` / `get` 经过一个跨进程闸门:临时目录下的 `fastpaper-arxiv.lock`,请求全程持锁(单连接),文件里记上次请求结束的时间戳,下一个请求至少等满 3 秒。锁文件打不开时不设闸,照常发出。
  - 只对 `arxiv.org` 主机生效;`FASTPAPER_ARXIV_URL` 指向的测试服务器或镜像不受约束。
  - PDF / e-print 下载走 `arxiv.org` 文件,不在条款范围内,不排队。
  - 429:有 `Retry-After` 按它等(超过 30 秒直接报错并写明秒数,不在调用里干等);没有则 3s、6s 后重试,共 3 次。
- 无结构化章节 API,全文靠 PDF 提取。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `max_results`(单次 ≤2000) |
| `--offset` | `start` |
| `--sort` | `sortBy=relevance` / `submittedDate`;**citations 不支持**,arXiv 不提供引用数 |
| `--order` | `sortOrder=ascending` / `descending` |
| `--year` | `search_query` 内 `submittedDate:[YYYY01010000 TO YYYY12312359]` |
| `--after` / `--before` | 同上,按给定日期构造区间 |
| `--author` | `search_query` 内 `au:"{name}"` |
| `--field` | `search_query` 内 `cat:{value}` |
| `--open-access` | 恒满足,不追加条件(arXiv 全部可免费阅读) |
