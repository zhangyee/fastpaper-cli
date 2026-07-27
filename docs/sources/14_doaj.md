# DOAJ

- **API 类型**: REST JSON
- **基础 URL**: `https://doaj.org/api`,文章检索端点 `/v4/search/articles/{query}`
- **查询串在路径段里,不在 query string 里。**因此空格必须编码成 `%20`:`+` 在路径段中是字面加号,`crispr+AND+bibjson.year:2024` 命中 0 条,而 `%20AND%20` 形式命中数千条。
- **认证**: 无需
- **能力**: search + download
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

无直接 PDF 端点:按 identifier 调搜索接口(`pageSize=1`)拿 `pdf_url` 再下载,见 `src/download.rs::pdf_bytes_doaj`;落盘后用 `fastpaper read papers/<id>.pdf` 提取文本。

## 注意

- 端点必须带 `/api` 前缀:`https://doaj.org/search/articles/...` 返回 403 HTML(2026-07 实测),须用 `https://doaj.org/api/v4/search/articles/...`。默认 base `https://doaj.org`(`FASTPAPER_DOAJ_URL` 可覆盖);`search()` 与 `pdf_bytes_doaj` 均在路径中拼 `/api/v4`。
- 429 → 指数退避重试(实现内置 3 次);5xx → 直接报错。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `pageSize`(≤100) |
| `--offset` | `page`,**必须是 `-n` 的整数倍** |
| `--author` | 查询路径内 `bibjson.author.name:{name}` |
| `--year` | 查询路径内 `bibjson.year:{year}` |
| `--open-access` | 恒满足(DOAJ 收录的全部是开放获取) |
| `--sort` | 不支持。`bibjson.year` 不可排序,`created_date` 是入库时间而非出版时间 |
| `--after` / `--before` | 不支持。记录只有年份没有日期,按日过滤只能悄悄放宽成整年 |
| `--field` | 不支持 |
