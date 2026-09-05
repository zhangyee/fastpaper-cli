# HAL

- **API 类型**: REST JSON(Solr 接口)
- **搜索 URL**: `https://api.archives-ouvertes.fr/search/`
- **认证**: 无需
- **能力**: search + get + download
- `get`:`q=halId_s:"{id}"`;DOI 用 `doiId_s:"{doi}"`(HAL 没有 by-id 路径)
- **实现**: `src/sources/hal.rs`(以代码为准)

## 搜索

`GET /search/?q={query}&rows={n}&wt=json&fl={字段列表}`。

- `fl` 需显式列出要返回的字段;实现请求:`halId_s,title_s,authFullName_s,abstract_s,doiId_s,publicationDateY_i,fileMain_s,uri_s`。
- 其他可用参数(实现未用):`start`(偏移)、`sort`(相关度 `score desc`、日期 `producedDateY_i desc`)、`fq`(Solr 过滤,如 `publicationDateY_i:[2018 TO *]`、`authFullName_s:"..."`、`domain_s:math`,多条件 ` AND ` 连接)。
- 不支持按引用数排序。

## 响应映射(→ Paper)

顶层 `response.docs[]`,总数在 `response.numFound`。同一字段可能是数组或字符串:

- `id`:`halId_s`(如 `hal-01234567`)
- `title_s` / `abstract_s` / `doiId_s`:数组时取第一个元素
- `authors`:`authFullName_s`(数组)
- `year`:`publicationDateY_i`
- `url`:`uri_s`
- `pdf_url`:`fileMain_s`(仅记录带全文文件时存在)
- `open_access`:恒 `true`;`venue` / `citations` / `fields`:无

## 下载

按 identifier 调搜索接口(`rows=1`)拿 `fileMain_s` 再下载,见 `src/download.rs::pdf_bytes_hal`;落盘后用 `fastpaper read papers/<id>.pdf` 提取文本。备用 PDF 直链模式:`https://hal.archives-ouvertes.fr/{halId}/document`(实现未用;metadata-only 或禁运记录返回非 200)。

## 注意

- 记录可能仅有元数据(无 `fileMain_s`),下载报 "No PDF URL found"。
- 429 → 指数退避重试(实现内置 3 次);5xx → 直接报错。
- 默认 base `https://api.archives-ouvertes.fr`,`FASTPAPER_HAL_URL` 可覆盖。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `rows`(≤10000) |
| `--offset` | `start` |
| `--author` | `q` 内 `authFullName_s:"{name}"` |
| `--year` | `fq=publicationDateY_i:{year}` |
| `--open-access` | `fq=openAccess_bool:true` |
| `--field` | `fq=domain_s:*{value}*`。`domain_s` 存的是 `0.sdv` / `1.sdv.bbm` 这类层级码,**不加通配符匹配不到** |
| `--sort` | `sort=publicationDateY_i asc|desc`;**citations 不支持** |
| `--after` / `--before` | 不支持。`publicationDateY_i` 只有年份粒度 |
