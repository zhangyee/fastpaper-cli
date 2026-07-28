# CORE

- **API 类型**: REST JSON(v3)
- **基础 URL**: `https://api.core.ac.uk`,搜索端点 `/v3/search/works`
- **认证**: 可选 `CORE_API_KEY`,header `Authorization: Bearer {key}`(提升 rate limit 和结果质量)
- **能力**: search + get + download
- **`CORE_API_KEY` 事实上必需**:匿名请求一律 429
- 检索路径要带尾斜杠:`/v3/search/works` 会 301 到 `/v3/search/works/`
- `--author` 用 `authors.name:"名字"`(**必须带引号**);写成 `authors:"名字"` 不是收窄而是**放大**结果集(同一查询 60460 → 874397),写成不带引号的 `authors.name:名字` 则命中 0
- `-n` 上限 **100**:200 就会 504
- `get`:`GET /v3/works/{identifier}`,返回裸 work 对象而非 `{results:[...]}` 信封
- `download`:`/v3/works/{id}/download` 会 302 到 `core.ac.uk` 上的文件,**API key 不能跟着跨主机**(带过去会被 400 拒)。实现改为两步:先带鉴权取记录,再单独取它的 `downloadUrl`(不带任何凭证)。`/v3/outputs/{id}/download` 已 404
- **实现**: `src/sources/core.rs`(以代码为准)

## 搜索

- 参数:`q`、`limit`(≤100)、`offset`。
- 日期过滤走 `filter` 参数(`publishedDate>=YYYY-MM-DD` / `publishedDate<=YYYY-MM-DD`)。
- 429 → 指数退避重试(最多 3 次);401/403 且带 key → 去掉 key 重试一次。

## 响应映射(results[] → Paper)

- `id` ← `id`(数字转字符串);`title` 为空的条目跳过;`authors` ← `authors[].name`
- `abstract` ← `abstract`;`doi` ← `doi`;`year` ← `publishedDate` 前 4 位(fallback `yearPublished`)
- `pdf_url` ← `downloadUrl`;`url` ← `links[0].url`;`fields` ← `fieldOfStudy`(单值字符串)
- `citations` ← `citationCount`;`open_access` 恒 `Some(true)`(CORE 专注 OA 内容);`venue` 恒 None
- 总数在 `totalHits`。

## 下载

- 无独立 PDF 端点:按标识符 search 取第一条的 `downloadUrl` 再下载(见 `src/download.rs::pdf_bytes_core`)。
- API 另有单篇详情 `GET /v3/works/{id}`(含 `fullText` 全文字段、`fullTextUrls`),当前实现未使用。

## 注意

- 端点必须带 `/v3` 前缀:`{base}/search/works`(缺 `/v3`)对真实 API 返回 404。`core.rs::search` 与 `download.rs::pdf_bytes_core` 均拼 `{base}/v3/search/works`。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` / `--offset` | `limit` / `offset` |
| `--author` | `q` 内 `authors:"{name}"` |
| `--year` | `q` 内 `yearPublished:{year}` |
| `--sort` / `--field` / `--open-access` | 不支持 |
