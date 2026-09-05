# CORE

- **API 类型**: REST JSON(v3)
- **基础 URL**: `https://api.core.ac.uk`,搜索端点 `/v3/search/works`
- **认证**: 可选 `CORE_API_KEY`,header `Authorization: Bearer {key}`(提升 rate limit 和结果质量)
- **能力**: search + get + download
- **`CORE_API_KEY` 事实上必需**:匿名请求一律 429
- 检索路径要带尾斜杠:`/v3/search/works` 会 301 到 `/v3/search/works/`
- `--author` 用 `authors.name:"名字"`(**必须带引号**);写成 `authors:"名字"` 不是收窄而是**放大**结果集(同一查询 60460 → 874397),写成不带引号的 `authors.name:名字` 则命中 0
- `-n` 上限 **100**:200 就会 504
- `get`:`GET /v3/works/{identifier}?exclude=fullText`,返回裸 work 对象而非 `{results:[...]}` 信封
- **search 和 get 都带 `exclude=fullText`**:两个端点默认都回整篇抽取文本,而代码里没有任何地方读它。实测单条 825K 字符;`limit=10` 时 424 KB vs 排除后 59 KB;单篇 1.22 MB vs 64 KB。排除后解析用到的字段一个不少
- `download`:`/v3/works/{id}/download` 和 `/v3/outputs/{id}/download` 都只是 302 到 `core.ac.uk/download/{id}.pdf`(官方文档写的是直接返回 PDF,实际不是;加 `?format=pdf` 或 `Accept: application/pdf` 也一样)。**API key 不能跟着跨主机**(带过去会被 400 拒)。实现为两步:先带鉴权取记录,再单独取它的 `downloadUrl`(不带任何凭证)。注意 `/v3/outputs/{id}` 里的 id 是 **output id**(`downloadUrl` 文件名里那个数字),不是 search 返回的 work id,拿 work id 去请求会 404
- **`core.ac.uk/download/*` 挂着 Cloudflare Interactive Challenge**,程序化下载在多数网络下必然 403,详见下文"下载"
- **实现**: `src/sources/core.rs`(以代码为准)

## 搜索

- 参数:`q`、`limit`(≤100)、`offset`。
- 日期过滤走 `filter` 参数(`publishedDate>=YYYY-MM-DD` / `publishedDate<=YYYY-MM-DD`)。
- 429 → 指数退避重试(最多 3 次);401/403 且带 key → 去掉 key 重试一次。

## 响应映射(results[] → Paper)

- `id` ← `id`(数字转字符串);`title` 为空的条目跳过;`authors` ← `authors[].name`
- `abstract` ← `abstract`;`doi` ← `doi`;`year` ← `publishedDate` 前 4 位(fallback `yearPublished`)
- `pdf_url` ← `downloadUrl`;`url` ← `links[0].url`;`fields` ← `fieldOfStudy`(单值字符串)
- `citations` ← `citationCount`;`open_access` 恒 `Some(true)`(CORE 专注 OA 内容);`venue` 恒 None
- 总数在 `totalHits`。

## 下载

- 无独立可用的 PDF 端点:按标识符 `get` 取记录,再下载它的 `downloadUrl`(见 `src/download.rs::pdf_bytes_core`)。
- `downloadUrl` 绝大多数落在 `core.ac.uk/download/{id}.pdf`(抽样 25 条:20 条在 core.ac.uk,5 条为空),少数直接指向 arxiv.org 之类的源仓库。

### Cloudflare 拦截(2026-09-05 实测)

- **现象**:`core.ac.uk/download/*` 和 `/works/*` 对所有非浏览器客户端返回 403,响应头 `cf-mitigated: challenge`,挑战页 `cType: 'interactive'`。真实浏览器也会停在勾选框上。
- **根因是 CORE 自己按路径配的 WAF 规则,不是 IP 信誉**:同一出口下 `/`、`/search/`、`/reader/{id}`、`/robots.txt` 都 200。换 UA、加请求头、换出口网络、带 API key 都无效;Interactive 是 Cloudflare 最严的挑战类型,文档明说需要访客交互,clearance cookie 绑定设备不可复用。
- 官方文档的"被挡时备用方式" `/v3/outputs/{id}/download` 只是 302 回到同一个被挡的 URL,形同失效。API 服务页各档位都没有提到下载放行。
- **报错行为**:`download.rs::fetch_limited` 把 4xx 当正常响应读回来(ureq `http_status_as_error(false)`),看到 `cf-mitigated: challenge` 时走 `challenged()`:明确说这是 Cloudflare bot 挑战、**不是 paywall**、客户端无法通过、应换源或用浏览器打开。`pdf_bytes_core` 再追加记录里的 DOI 和 `fastpaper download <DOI>`,让调用方直接换源。没有该头的普通 403 仍走 `refused()`,照旧说"分不清 paywall 和 bot block"。
- **registry 里 core 的 `download` 保持 `true`,是有意为之**:这是环境层面的拦截而非能力缺失,对网络正常的用户它能用;对 AI 调用方来说报错信息已足够让它自行换源。
- **禁区**:不要走 `fileserver-az.core.ac.uk/download/{id}.pdf`,CORE 政策明令禁止,会封 API key;也不要复用浏览器 cookie 或用打码服务过挑战。
- **未用上的替代路径**:(1) 记录自带 `fullText`(CORE 用 PDFBox 抽的纯文本),走 `api.core.ac.uk` 不经 Cloudflare,目前被 `exclude=fullText` 排除,要用需另开出口;(2) `/v3/outputs/{id}` 的 `sourceFulltextUrls` 指向源仓库,抽样 8/8 都有非 CORE 地址(hdl.handle.net、doi.org、机构库、europepmc),直链 PDF 抽测 3 个有 2 个可直接下载,可作下载兜底。

## 注意

- 端点必须带 `/v3` 前缀:`{base}/search/works`(缺 `/v3`)对真实 API 返回 404。`core.rs::search` 拼 `{base}/v3/search/works/`,`get_by_id` 拼 `{base}/v3/works/{id}`。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` / `--offset` | `limit` / `offset` |
| `--author` | `q` 内 `authors:"{name}"` |
| `--year` | `q` 内 `yearPublished:{year}` |
| `--sort` / `--field` / `--open-access` | 不支持 |
