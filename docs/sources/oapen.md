# OAPEN Library

- **API 类型**: DSpace REST JSON
- **搜索 URL**: `https://library.oapen.org/rest/search`
- **认证**: 无需
- **能力**: search + get(按 handle)+ download
- **实现**: `src/sources/oapen.rs`(以代码为准)

同行评审的开放获取学术专著(OAPEN 基金会,荷兰),人文社科为主。同一基金会的 DOAB 只是书目、文件在出版社,不接。官方说明页被 Cloudflare 挡(WebFetch 403、浏览器是安全验证页),以下全部以 2026-09-19 实测为准。

## 搜索

`GET /rest/search?query={q}&limit={n}&offset={k}&expand=metadata,bitstreams`

- `query` 接 Solr 风格子句:`dc.type:book`、`dc.date.issued:2023`、`dc.contributor.author:"Latour"`、`dc.title:…`;`AND` / `OR` / `NOT` 可用;`dc.type` 检索不分大小写。
- **索引里混有资助方记录**(`dc.type:grantor`,无标题):`machine learning` 前 100 条 book 68 / chapter 32 / grantor 4。实现固定追加 `AND NOT dc.type:grantor`,保留书与章节。
- 按「书或章节」`OR` 过滤会大幅改变排序(前 10 条变成 9 个章节),所以用 `NOT grantor` 而不是 `OR`。
- `limit=500` 耗时 19 s,而 API 请求总时限 30 s:`-n` 上限 100。

## 单篇与文件

- `GET /rest/handle/<handle>?expand=metadata,bitstreams` → 单个 item。
- PDF:`bitstreams[]` 里 `mimeType == application/pdf` 的 `retrieveLink`(如 `/rest/bitstreams/<uuid>/retrieve`),实测 14.8 MB 真 PDF。

## 响应映射(→ Paper)

`metadata[]` 是 `{key, value}` 列表。`id` = `handle`;`title` = `dc.title`;`authors` = `dc.contributor.author`,为空时用 `dc.contributor.editor` 并加 ` (ed.)`;`abstract` = `dc.description.abstract`;`year` = `dc.date.issued`;`doi` = `oapen.identifier.doi`;`venue` = `publisher.name`;`fields` = `dc.subject.classification` 的末段;`open_access` = true。章节标题自带 `Chapter` 前缀,偶有同一章节挂在两个 handle 下。
