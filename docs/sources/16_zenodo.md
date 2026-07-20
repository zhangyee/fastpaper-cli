# Zenodo

- **API 类型**: REST JSON
- **基础 URL**: `https://zenodo.org/api`
- **认证**: 无需
- **能力**: search + download + read;CERN 运营的通用存储库(论文/数据集/软件)
- **实现**: `src/sources/zenodo.rs`(以代码为准)

## 搜索

`GET /api/records`,参数:`q`(Elasticsearch 查询语法)、`size`、`page`、`type=publication`(只取论文)、`sort`(如 `mostrecent`)、`access_right=open`。

- 日期过滤写进查询串:`... AND publication_date:[2020-01-01 TO *]`。
- 不支持按引用数排序。单条详情:`GET /api/records/{id}`。

## 响应映射(→ Paper)

顶层 `hits.hits[]`,总数在 `hits.total`。每条 hit:

- `id`:`hit.id`(数字 record id;落地页模式 `https://zenodo.org/record/{id}`)
- `title` / `description`(→ abstract,可能含 HTML 标签,实现未剥离)/ `creators[].name` / `publication_date`(取前 4 位为 year)/ `access_right`:均在 `hit.metadata` 下
- `doi`:hit 顶层 `doi`;Zenodo 自身 DOI 形如 `10.5281/zenodo.{record_id}`
- `url`:`hit.links.html`
- `pdf_url`:`hit.files[]` 中 `key` 以 `.pdf` 结尾者的 `links.self`
- `open_access`:`metadata.access_right == "open"`
- `venue` / `citations`:无

## 下载

无直接 PDF 端点:按 identifier 调 `/api/records?q={id}&size=1&type=publication` 拿 `pdf_url` 再下载,见 `src/download.rs::download_zenodo`;read = 下载后本地提取 PDF 文本。

## 注意

- 端点必须带 `/api`:`https://zenodo.org/records?q=...` 返回 404 HTML(2026-07 实测),须用 `/api/records`。默认 base `https://zenodo.org`(`FASTPAPER_ZENODO_URL` 可覆盖);`search()` 与 `download_zenodo` 均拼 `/api`。
- 429 → 指数退避重试(实现内置 3 次);5xx → 直接报错。
