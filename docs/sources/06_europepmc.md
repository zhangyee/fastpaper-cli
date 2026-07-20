# Europe PMC

- **API 类型**: REST JSON
- **基础 URL**: `https://www.ebi.ac.uk`
- **认证**: 无需
- **能力**: search only(当前实现无 download / read)
- **实现**: `src/sources/europepmc.rs`(以代码为准)

## 搜索

- 请求:`GET {base}/search?query={encode(query)}&pageSize={max_results}&format=json&resultType=core`
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
