# DataCite

- **API 类型**: REST JSON:API
- **搜索 URL**: `https://api.datacite.org/dois`
- **认证**: 无需
- **能力**: search + get(无 download)
- **实现**: `src/sources/datacite.rs`(以代码为准)

DataCite 是 Crossref 覆盖不到的那部分 DOI 注册机构:数据集、软件,以及**大量高校学位论文和机构库存缴**。和 `crossref` 是互补关系,两者合起来覆盖了已注册 DOI 空间的大部分。

## 搜索

`GET /dois?query={q}&page[size]={n}&page[number]={p}`

- `query` 是 Lucene 语法,可以拼字段条件。
- ⚠️ **`publication-year=` 参数会被静默忽略。**2026-09-05 实测:`&publication-year=2020` 返回的 5 条是 2021/2018/2020/2018/2020;改用 `query=... AND publicationYear:2020` 后 5 条全是 2020。所以年份**必须**走 query,不能走那个同名参数。
- 翻页是页码,`--offset` 必须是 `-n` 的整数倍。

## 响应映射(→ Paper)

顶层 `data[]`,每条的字段在 `.attributes`。

- `id` / `doi`:`attributes.doi`(用 DOI 当 id)
- `title`:`attributes.titles[0].title`
- `authors`:`attributes.creators[].name`(个人与机构作者共用 `name`)
- `abstract_text`:`attributes.descriptions[]` 里 `descriptionType == "Abstract"` 那条,没有则退回第一条
- `year`:`attributes.publicationYear`
- `venue`:`attributes.publisher`(**注意:部分记录该字段是对象而非字符串,且存在源数据编码损坏,如 `Universit� IUAV`**)
- `url`:`attributes.url`(指向仓储落地页)
- `fields`:`attributes.types.resourceTypeGeneral`(`Text` / `Dataset` / `Software` 等)
- `pdf_url` / `citations` / `open_access`:**全部 None**——DataCite 只登记元数据,文件在各自仓储,记录也不说是否可免费读

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `page[size]` |
| `--offset` | `page[number]` 页码;必须是 `-n` 的整数倍 |
| `--year` | `query` 内 ` AND publicationYear:{y}`(**不是** `publication-year=`,那个会被忽略) |
| 其余全部 | **不支持**(未实测通过,故未声明) |
