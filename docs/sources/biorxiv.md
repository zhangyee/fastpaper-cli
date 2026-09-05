# BioRxiv

- **API 类型**: REST JSON
- **基础 URL**: `https://api.biorxiv.org`(下载 `https://www.biorxiv.org`)
- **认证**: 无需
- **能力**: search + get + download
- `get`:`GET /details/biorxiv/{doi}`
- **实现**: `src/sources/biorxiv.rs`(以代码为准)

## 搜索

- API **不支持关键词/全文搜索**,只能按日期范围浏览。
- 请求:`GET {base}/details/biorxiv/{start_date}/{end_date}/0`,日期格式 `YYYY-MM-DD`,末段 `0` 为 cursor。
- 时间窗固定为**最近 30 天**(start = 今天 - 30d,end = 今天);cursor 固定 `0`,不翻页。
- 关键词过滤在**本地**执行:title 或 abstract(小写)包含 query 即保留;再 `take(max_results)`。

## 响应映射(collection[] → Paper)

- `id` / `doi`:`doi`(空则 `doi=None`)
- `version`:`version`(缺省 `"1"`),用于拼 URL
- `authors`:`authors` 按 `;` 分割、trim
- `abstract_text`:`abstract`
- `year`:`date` 前 4 位
- `fields`:`[category]`
- `url`:`https://www.biorxiv.org/content/{doi}v{version}`
- `pdf_url`:`https://www.biorxiv.org/content/{doi}v{version}.full.pdf`
- `venue`:固定 `"bioRxiv preprint"`;`open_access`:恒 `true`;`citations`:无

## 下载

- PDF:`{dl_base}/content/{identifier}v1.full.pdf`(版本**硬编码为 v1**),identifier 即 DOI。
- read:下载 PDF 后本地提取文本,无结构化章节。

## 注意

- HTTP 429 退避重试(最多 3 次);5xx 直接报错。
- API 无排序/引用数/同行评审等过滤能力。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `--year` / `--after` / `--before` | **决定 `/details/biorxiv/{start}/{end}/{cursor}` 的日期区间**——这是本源唯一能原生支持的过滤 |
| `-n` / `--offset` | 本地切片。游标每页 30 条,最多翻 20 页,凑够结果或窗口耗尽即停 |
| `--open-access` | 恒满足(预印本全部开放) |
| `--sort` / `--author` / `--field` | 不支持 |
