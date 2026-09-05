# ERIC

- **API 类型**: REST JSON(Solr 包装)
- **搜索 URL**: `https://api.ies.ed.gov/eric/`
- **认证**: 无需
- **能力**: search + get + download
- **实现**: `src/sources/eric.rs`(以代码为准)

美国教育部教育研究索引。教育学在本 CLI 其余 22 个源里基本是空白。

## 搜索

`GET /?search={q}&format=json&rows={n}&start={offset}&fields={字段列表}`

- `fields` **必须显式列出**,否则返回的字段很少。实现请求:`id,title,author,description,publicationdateyear,source,subject,url,peerreviewed,e_fulltextauth`。
- `start` 是**真正的记录偏移**(不是页码),所以 `--offset` 任意值都合法。

## 响应映射(→ Paper)

顶层 `response.docs[]`,总数在 `response.numFound`。

- `id`:`id`(ERIC 编号,`ED...` 报告 / `EJ...` 期刊文章)
- `title` / `abstract_text`:`title` / `description`
- `authors`:`author[]`
- `year`:`publicationdateyear`
- `venue`:`source`(**部分记录没有此字段**)
- `url`:`url`(**同样部分记录缺失**)
- `fields`:`subject[]`
- `pdf_url`:`e_fulltextauth == 1` 时为 `https://files.eric.ed.gov/fulltext/{id}.pdf`,否则 None
- `open_access`:同上,`e_fulltextauth == 1` 才是 `Some(true)`
- `doi`:**恒 None**,ERIC 不登记也不携带 DOI

## 注意

- `e_fulltextauth` 是唯一判断「ERIC 自己有没有这个文件」的依据;为 0 的记录去拼 PDF 路径会 404,所以两个字段都严格跟随它。
- fixture 特意选了同时包含 `e_fulltextauth` 0 和 1 的查询,否则 `tests/field_capabilities.rs` 会正确地报「声明了 pdf_url 但 fixture 产不出」。

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `rows` |
| `--offset` | `start`(真实记录偏移,任意值合法) |
| 其余全部 | **不支持**(未实测通过,故未声明) |
