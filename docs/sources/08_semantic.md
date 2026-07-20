# Semantic Scholar

- **API 类型**: REST JSON(Graph API v1)
- **基础 URL**: `https://api.semanticscholar.org`(端点自带 `/graph/v1` 前缀)
- **认证**: 可选 `SEMANTIC_SCHOLAR_API_KEY`,header `x-api-key`(rate limit 1→100 req/s)
- **能力**: search + download + read
- **实现**: `src/sources/semantic.rs`(以代码为准)

## 搜索

- 端点:`GET /graph/v1/paper/search`,参数 `query`、`limit`、`offset`、`fields`。
- `fields` 取值(代码 `FIELDS` 常量):`title,abstract,year,citationCount,authors,url,publicationDate,externalIds,fieldsOfStudy,openAccessPdf,venue`。
- 年份过滤参数 `year` 支持范围写法:`2019`、`2016-2020`、`2010-`、`-2015`;`fieldsOfStudy` 参数可按学科过滤。
- 搜索 API 不支持自定义排序(仅 relevance)。

## 单篇查询

- `GET /graph/v1/paper/{id}`,`fields` 同上;`id` 支持 S2 hash 及 `DOI:`、`ARXIV:`、`PMID:`、`URL:` 前缀。404 → `Ok(None)`。

## 响应映射(data[] → Paper)

- `id` ← `paperId`;`authors` ← `authors[].name`;`doi` ← `externalIds.DOI`
- `pdf_url` ← `openAccessPdf.url`;若为 `arxiv.org/abs/` 链接则改写为 `/pdf/`
- `citations` ← `citationCount`;`fields` ← `fieldsOfStudy`;`title`/`abstract`/`year`/`url`/`venue` 直取
- `open_access` ← `openAccessPdf` 对象是否存在

## 下载

- 无独立 PDF 端点:先 `get_by_id` 拿 `openAccessPdf.url`,再直接下载(见 `src/download.rs::download_semantic`)。

## 注意

- 限频严格:代码含进程级节流、指数退避(带 jitter)、`Retry-After` 解析,最多重试 5 次,超限返回 `Err("rate limited …")`。
- 403 且带 key 时:去掉 key 重试一次(key 失效时不至于完全不可用)。
