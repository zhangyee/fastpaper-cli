# Europe PMC

- **API 类型**: REST JSON
- **基础 URL**: `https://www.ebi.ac.uk`
- **认证**: 无需
- **能力**: search + get + download
- `get`:无按 id 的路径,用字段限定检索——`PMCID:{id}` / `DOI:"{doi}"` / `EXT_ID:{pmid}+AND+SRC:MED`
- `download`:`fullTextUrlList` 里 `documentStyle=="pdf"` 的那条,形如 `https://europepmc.org/articles/PMC*?pdf=render`(实测 `application/pdf`)
- 另有 `/{id}/fullTextXML`、`/{id}/supplementaryFiles`(本 CLI 暂未使用)
- **实现**: `src/sources/europepmc.rs`(以代码为准)

## 搜索

- 请求:`GET {base}/europepmc/webservices/rest/search?query={encode(query)}&pageSize={n}&page={p}&format=json&resultType=core`
- **`{base}` 是裸主机名,REST 服务在 `/europepmc/webservices/rest` 下。**早先文档记的 `{base}/search` 在生产环境返回 404;测试当时用 `server.url()` 当 base 且匹配任意路径,所以没暴露。
- query 原样传入(Europe PMC 检索语法如 `AUTH:`、`PUB_YEAR:`、`OPEN_ACCESS:y` 可写在 query 里,但 CLI 不做本地拼接)。
- 无 cursorMark / sort / offset 逻辑。

## 响应映射(resultList.result[] → Paper)

- `id`:`id`(原样,不加前缀)
- `title`:`title`
- `authors`:`authorList.author[].fullName`
- `abstract_text`:`abstractText`
- `year`:`pubYear`
- `doi`:`doi`
- `venue`:`journalTitle`
- `citations`:`citedByCount`(Europe PMC **提供**引用数)
- `open_access`:`isOpenAccess == "Y"`
- `url`:`fullTextUrlList.fullTextUrl[0].url`(取首个)
- `pdf_url`:无;`fields`:空;`source`:`"europepmc"`

## 注意

- HTTP 429 退避重试(最多 3 次);5xx 直接报错。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | `pageSize`(≤1000) |
| `--offset` | `page`,**必须是 `-n` 的整数倍**(本源按页翻,没有 offset) |
| `--author` | `query` 内 `AUTH:"{name}"` |
| `--year` | `query` 内 `PUB_YEAR:{year}` |
| `--after` / `--before` | `query` 内 `FIRST_PDATE:[{from} TO {to}]` |
| `--open-access` | `query` 内 `OPEN_ACCESS:y` |
| `--sort` | `query` 末尾追加 `sort_date:y` / `sort_cited:y`。**只能降序**,`--order asc` 直接报错 |
| `--patents` | `query` 内追加 `SRC:PAT`,收窄到 EPO 专利子集(原生过滤,无翻页代价) |
| `--field` | 不支持 |
