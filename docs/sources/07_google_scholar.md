# Google Scholar

- **API 类型**: HTML 爬取(无官方 API)
- **基础 URL**: `https://scholar.google.com`
- **认证**: 无需
- **能力**: search
- **实现**: `src/sources/scholar.rs`(以代码为准)

## 搜索

- 请求:`GET {base}/scholar?q={encode(query)}&start=0&hl=en&as_sdt=0,5&num={max_results}`
- User-Agent 从 3 个桌面浏览器 UA 池中按时间戳取模选一(每次请求换一个,但**单次请求、无重试、无随机延迟**)。
- 状态码 429 / 403 → 直接返回明确错误(限频 / 被封)。
- CAPTCHA 检测:响应含 `gs_captcha_f` 或 `name="captcha"` → 返回错误。

## 响应映射(用 scraper 解析 HTML → Paper)

- 结果条目选择器:`div.gs_ri`
- `title` + `url`:`h3.gs_rt a`(文本 / `href`)
- `id`:`url` 按 `/` 分割取最后一段
- `authors` / `year` / `venue`:`div.gs_a` 文本按 ` - ` 分段;首段按 `,` 拆作者;4 位数(1900–2100)为 year;中段去掉年份为 venue
- `abstract_text`:`div.gs_rs`
- `doi`:恒 `None`(不解析);`pdf_url` / `citations` / `open_access`:均无;`fields`:空
- `source`:`"scholar"`(**不是** `"google_scholar"`)

## 注意

- 不支持 download / read。
- 引用数不可靠,统一不输出。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` / `--offset` | `num` / `start` |
| 其余全部 | 不支持。这是 HTML 抓取而非 API,加过滤条件收益低、易碎 |
