# zbMATH Open

- **API 类型**: REST JSON
- **搜索 URL**: `https://api.zbmath.org/v1/document/_search`
- **认证**: 无需(2021 年起全面开放)
- **能力**: search + get(无 download)
- **实现**: `src/sources/zbmath.rs`(以代码为准)

数学文摘与评论库。arXiv 只有预印本,zbMATH 覆盖 1868 年至今**已发表**的数学文献并附 MSC 分类与署名评论,是真正的互补而非重叠。

## 搜索

`GET /v1/document/_search?search_string={q}&results_per_page={n}&page={p}`

- 字段限定写在 `search_string` 里,用前缀语法:作者 `au:Yano`(实测生效),多条件用 ` & ` 连接。
- 翻页是页码,`--offset` 必须是 `-n` 的整数倍。

## 响应映射(→ Paper)

顶层 `result[]`;`status.execution_bool` 为 false 时表示查询失败(HTTP 仍是 200)。

- `id`:`result[].id`(**整数**,转成字符串)
- `title`:`title.title`(**比字段名多一层嵌套**)
- `authors`:`contributors.authors[].name`
- `year`:`year`(**JSON 字符串**,不是数字)
- `doi`:`links[]` 里 `type == "doi"` 那一项的 `identifier`——没有独立 doi 字段
- `venue`:`source.series[0].title`
- `fields`:`msc[].code`(MSC 分类号,如 `53C07`)
- `url`:`zbmath_url`
- `abstract_text` / `pdf_url` / `citations` / `open_access`:**全部为 None**

## 注意

- 搜索响应里**没有摘要**(zbMATH 出的是评论不是作者摘要),也没有全文和引用数,所以 `FieldCaps::NONE`。

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `results_per_page` |
| `--offset` | `page` 页码;必须是 `-n` 的整数倍 |
| `--author` | `search_string` 内 ` & au:{name}` |
| 其余全部 | **不支持** |
