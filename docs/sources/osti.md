# OSTI.GOV

- **API 类型**: REST JSON
- **搜索 URL**: `https://www.osti.gov/api/v1/records`
- **认证**: 无需
- **能力**: search + get + download
- **实现**: `src/sources/osti.rs`(以代码为准)

美国能源部科研产出:技术报告、学位论文、会议论文、数据集。**技术报告这类灰色文献在其余源里完全缺席。**

## 搜索

`GET /api/v1/records?q={q}&rows={n}&page={p}`

- 日期过滤:`publication_date_start` / `publication_date_end`,格式是 **`MM/DD/YYYY`**(不是 ISO),实测生效。
- 排序:`sort=publication_date desc|asc`,实测生效。
- 翻页是页码,`--offset` 必须是 `-n` 的整数倍。

## 响应映射(→ Paper)

响应是**裸数组**(`get` 单条时是裸对象),没有外层包装。

- `id`:`osti_id`
- `title` / `abstract_text`:`title` / `description`
- `authors`:`authors[]`,**名字后面跟着方括号里的单位**(`"Yan, Feng [Arizona State University, Tempe, AZ]"`),要按 `" ["` 切掉
- `year`:`publication_date` 前 4 位
- `doi`:`doi`
- `url`:`links[]` 里 `rel == "citation"` 那条的 `href`
- `pdf_url`:`links[]` 里 `rel == "fulltext"` 那条的 `href`(形如 `https://www.osti.gov/servlets/purl/{id}`)
- `open_access`:是否存在 fulltext 链接
- `venue`:`journal_name`,回退 `product_type`
- `citations` / `fields`:无

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `rows` |
| `--offset` | `page` 页码;必须是 `-n` 的整数倍 |
| `--year` | `publication_date_start=01/01/{y}` + `publication_date_end=12/31/{y}` |
| `--after` / `--before` | 同上两个参数,ISO 日期转成 `MM/DD/YYYY` |
| `--sort` | `sort=publication_date desc|asc`;**citations 不支持** |
| `--author` / `--field` / `--open-access` | **不支持**(未实测通过,故未声明) |
