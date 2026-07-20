# DOAJ

- **API 类型**: REST JSON
- **基础 URL**: `https://doaj.org/api`
- **认证**: 无需
- **能力**: search + download + read;内容全部开放获取
- **实现**: `src/sources/doaj.rs`(以代码为准)

## 搜索

`GET /api/search/articles/{编码后的查询}`(查询在路径中),参数:`page`(从 1 起)、`pageSize`(最大 100)、`sort`(如 `created_date:desc`)。

- 查询支持 Lucene 语法,可按字段限定并用 ` AND ` 连接:`bibjson.year:2020`、`bibjson.year:[2018 TO *]`、`bibjson.author.name:"..."`、`bibjson.subject.term:"..."`。
- 不支持按引用数排序;无需 OA / peer-reviewed 过滤(DOAJ 全部 OA 且期刊均同行评审)。

## 响应映射(→ Paper)

顶层 `results[]`,总数在 `total`。每条 item:

- `id`:`item.id`(DOAJ article id);落地页模式 `https://doaj.org/article/{id}`(实现未用)
- `title` / `abstract` / `year`(字符串,需 parse)/ 作者 `author[].name`:均在 `item.bibjson` 下
- `doi`:`bibjson.identifier[]` 中 `type=="doi"` 的 `id`
- `venue`:`bibjson.journal.title`
- `url` 与 `pdf_url`:`bibjson.link[]` 中 `type` 含 `fulltext` 的首个 `url`(实现两者取同一链接)
- `citations`:无;`open_access`:恒 `true`

## 下载

无直接 PDF 端点:按 identifier 调搜索接口(`pageSize=1`)拿 `pdf_url` 再下载,见 `src/download.rs::download_doaj`;read = 下载后本地提取 PDF 文本。

## 注意

- 端点必须带 `/api` 前缀:`https://doaj.org/search/articles/...` 返回 403 HTML(2026-07 实测),须用 `https://doaj.org/api/search/articles/...`。默认 base `https://doaj.org`(`FASTPAPER_DOAJ_URL` 可覆盖);`search()` 与 `download_doaj` 均在路径中拼 `/api`。
- 429 → 指数退避重试(实现内置 3 次);5xx → 直接报错。
