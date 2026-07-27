# OpenAIRE

- **API 类型**: REST JSON(XML 转 JSON 风格:文本值在 `$`,属性在 `@classid` 等)
- **基础 URL**: `https://api.openaire.eu`,搜索端点 `/graph/v3/research-products`
- **已从 legacy 的 `/search/researchProducts` 迁移。**两者响应结构完全不同:legacy 把内容嵌在 `response.results.result[].metadata["oaf:entity"]["oaf:result"]` 下,v3 是扁平的 `{header, results[]}`。
- **认证**: 无需
- **能力**: search
- **实现**: `src/sources/openaire.rs`(以代码为准)

## 搜索

- 参数:`keywords`、`size`(≤50)、`page`(从 1 起)、`format=json`。
- 日期过滤:`fromPublicationDate` / `toPublicationDate`(YYYY-MM-DD);OA 过滤:`OA=true`。

## 响应映射(response.results.result[] → Paper)

- 元数据在深层嵌套 `metadata."oaf:entity"."oaf:result"` 下。
- title、creator、description、pid 均可能是单对象或数组,需两态处理。
- `title`:数组时优先 `@classid == "main title"` 的项的 `$`,否则取第一个;为空跳过
- `authors` ← `creator[].$`;`abstract` ← `description` 中第一个非空 `$`
- `doi` ← `pid[]` 中 `@classid == "doi"` 的 `$`
- `year` ← `dateofacceptance.$` 前 4 位;`id` ← `header."dri:objIdentifier".$`
- 当前未映射(恒 None/空):`url`、`pdf_url`、`venue`、`citations`、`fields`、`open_access`

## 注意

- 响应里 `children.instance[].webresource[].url.$` 含落地页/PDF 链接、`bestaccessright` 的 `@classid == "OPEN"` 表示 OA,当前实现均未提取。
- 元数据源:下载需借助 DOI 走其他源。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `pageSize` |
| `--offset` | `page`,**必须是 `-n` 的整数倍** |
| `--author` | `authorFullName` |
| `--year` | `publicationYear` |
| `--after` / `--before` | `fromPublicationDate` / `toPublicationDate` |
| `--open-access` | `accessRightLabel="Open Access"`(**含空格的值必须加双引号**,否则 API 拒绝) |
| `--field` | `fos`,取值来自 OpenAIRE 的学科词表;传错时 API 会回列全部合法值 |
| `--sort` | `sortBy={relevance|publicationDate|citationCount} {ASC|DESC}` |
