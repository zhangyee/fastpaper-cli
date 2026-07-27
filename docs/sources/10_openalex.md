# OpenAlex

- **API 类型**: REST JSON
- **基础 URL**: `https://api.openalex.org`
- **认证**: 无需;`mailto` 参数进入 polite pool,取 `FASTPAPER_EMAIL`(fallback 硬编码项目邮箱)
- **能力**: search + get
- **实现**: `src/sources/openalex.rs`(以代码为准)

## 搜索

- `GET /works`,参数:`search`、`per_page`(≤200)、`page`(从 1 起)、`mailto`。
- 排序:`sort=publication_date:desc` / `cited_by_count:desc`(`:asc` 反向);默认按 relevance。
- 过滤:`filter` 逗号分隔:`publication_year:YYYY`、`from-publication-date:YYYY-MM-DD`、`to-publication-date:YYYY-MM-DD`、`is_oa:true`、`authorships.author.display_name.search:{name}`、`concepts.display_name.search:{field}`。

## 响应映射(results[] → Paper)

- `id` ← `id` 去掉前缀 `https://openalex.org/`(得 `W2741809807` 形式);`title` 为空的条目跳过
- `authors` ← `authorships[].author.display_name`;`doi` ← `doi` 去掉 `https://doi.org/` 前缀
- **摘要是倒排索引** `abstract_inverted_index`(词 → 位置列表),需按位置排序重建为文本
- `year` ← `publication_year`;`citations` ← `cited_by_count`;`open_access` ← `open_access.is_oa`
- `url` ← `primary_location.landing_page_url`(fallback `id`);`venue` ← `primary_location.source.display_name`
- `pdf_url` ← `primary_location.pdf_url`;缺失且 `is_oa` 为 true 时用 `open_access.oa_url`
- `fields` ← `concepts[].display_name` 取前 5 个
- 总数在 `meta.count`。

## 注意

- 元数据源:下载需自行使用搜索结果里的 `pdf_url`。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `per_page`(≤100) |
| `--offset` | `page`,**必须是 `-n` 的整数倍** |
| `--author` | `filter=raw_author_name.search:{name}` |
| `--year` | `filter=publication_year:{year}` |
| `--after` / `--before` | `filter=from_publication_date:` / `to_publication_date:` |
| `--open-access` | `filter=is_oa:true` |
| `--field` | `filter=concepts.id:{id}`。**必须是 concept ID(如 `C154945302`),不是名字**——`concepts.display_name.search` 与 `primary_topic.field.display_name` 均被 API 拒绝 |
| `--sort` | `sort=relevance_score` / `publication_date` / `cited_by_count` + `:asc|desc` |
