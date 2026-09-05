# Unpaywall

- **API 类型**: REST JSON
- **基础 URL**: `https://api.unpaywall.org/v2`
- **认证**: **必需**环境变量 `UNPAYWALL_EMAIL`(免费;API 条款要求提供联系邮箱)
- **能力**: get
- **实现**: `src/sources/unpaywall.rs`(以代码为准)

## 查找

`GET /v2/{doi}?email={UNPAYWALL_EMAIL}`,一次一个 DOI,响应为单个 JSON 对象(非列表)。

- 未设置 `UNPAYWALL_EMAIL` → 直接报错并提示 `export UNPAYWALL_EMAIL="your@email.com"`。
- 404 → DOI 不存在;422 → email 无效。
- 限频:官方上限约 100,000 次/天。

## 响应映射(→ Paper)

- `id` / `doi`:`doi`
- `title`:`title`;`year`:`year`;`venue`:`journal_name`
- `authors`:`z_authors[]` 的 `given + family` 拼接,为空回退 `raw_author_name`
- `url`:`doi_url`
- `pdf_url`:`best_oa_location.url_for_pdf`(空串视为无)。备选:遍历 `oa_locations[]` 的 `url_for_pdf` / `url`(当前实现未做)
- `open_access`:`is_oa`
- `abstract` / `citations` / `fields`:Unpaywall 不提供

## 注意

- Unpaywall 是 OA 解析器而非内容托管方,不提供下载;用返回的 `pdf_url` 到别处获取(如 `fastpaper download semantic <DOI>`)。
- `main.rs` 把 search 子命令的 query 直接当 DOI 传给 `lookup_doi`;默认 base `https://api.unpaywall.org`,`FASTPAPER_UNPAYWALL_URL` 可覆盖。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| —— | 本源没有检索能力,只按 DOI 解析。走 `fastpaper get unpaywall <DOI>` |
