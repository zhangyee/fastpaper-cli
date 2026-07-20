# OpenAlex

- **API 类型**: REST JSON
- **基础 URL**: `https://api.openalex.org`
- **认证**: 无需;`mailto` 参数进入 polite pool,取 `FASTPAPER_EMAIL`(fallback 硬编码项目邮箱)
- **能力**: search only(仅元数据,不支持 download / read)
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
