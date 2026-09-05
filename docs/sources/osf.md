# OSF Preprints

- **API 类型**: REST JSON:API
- **搜索 URL**: `https://api.osf.io/v2/preprints/`
- **认证**: 无需
- **能力**: search + get + download
- **实现**: `src/sources/osf.rs`(以代码为准)

一个接口覆盖 OSF 上约 32 个预印本社区(2026-09-05 实测枚举):`psyarxiv` `socarxiv` `edarxiv` `ecoevorxiv` `engrxiv` `metaarxiv` `thesiscommons` `lawarxiv` `marxiv` `paleorxiv` `africarxiv` `agrixiv` `mediarxiv` `sportrxiv` `mindrxiv` `nutrixiv` `frenxiv` `arabixiv` `indiarxiv` `inarxiv` `bodoarxiv` `biohackrxiv` `ecsarxiv` `lissa` `coppreprints` `acctrt` 等。这是本 CLI 唯一覆盖心理学、社会学、教育学、法学预印本的源。

## 搜索

`GET /v2/preprints/?filter[title]={q}&page[size]={n}&page={p}&embed=contributors&embed=provider`

- **没有全文检索。**`filter[q]` 返回 400(`'q' is not a valid field for this endpoint`),`/v2/search/preprints/` 是 404。query 只能匹配标题。
- `embed=contributors` 必须带:作者在 JSON:API relationship 里,不 embed 时作者列表**静默为空**。
- 翻页是页码(`page`),不是记录偏移,所以 `--offset` 必须是 `-n` 的整数倍。

## 响应映射(→ Paper)

顶层 `data[]`;`links.next` 非空表示还有下一页。

- `id`:`data[].id`(如 `2s7pw_v2`,带版本后缀)
- `title` / `abstract_text`:`attributes.title` / `attributes.description`
- `authors`:`embeds.contributors.data[].embeds.users.data.attributes.full_name`
- `doi`:**`attributes.doi` 几乎恒为 null**,真正的 DOI 要从 `links.preprint_doi`(`https://doi.org/10.31234/osf.io/...`)剥掉前缀得到
- `year`:`attributes.date_published` 前 4 位
- `venue`:`embeds.provider.data.attributes.name`(如 `PsyArXiv`)
- `fields`:`attributes.subjects` 是**分类路径数组的数组**,取每条路径的最后一个 `text`(叶子),祖先节点丢弃
- `pdf_url`:`https://osf.io/download/{id}/`(接口不给文件 URL,这是稳定公开路径)
- `open_access`:恒 `true`

## 下载

`https://osf.io/download/{id}/`,见 `src/download.rs::pdf_bytes_osf`。注意文件在 `osf.io`,API 在 `api.osf.io`,所以 `pdf_default_base` 与 `default_base` 是分开的。

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `page[size]` |
| `--offset` | `page` 页码;必须是 `-n` 的整数倍,否则报错 |
| `--field` | `filter[provider]`(取 provider id,如 `socarxiv`) |
| `--year` | `filter[date_published][gte]={y}-01-01` + `[lte]={y}-12-31` |
| `--after` / `--before` | `filter[date_published][gte]` / `[lte]` |
| `--sort` | `sort=-date_published` / `date_published`;**citations 不支持** |
| `--open-access` | 恒满足(全部预印本都开放),不发参数 |
| `--author` | **不支持**,接口没有作者过滤 |
