# NASA NTRS

- **API 类型**: REST JSON
- **搜索 URL**: `https://ntrs.nasa.gov/api/citations/search`
- **认证**: 无需
- **能力**: search + get + download
- **实现**: `src/sources/ntrs.rs`(以代码为准)

NASA 技术报告服务器:航空航天报告、会议论文、承包商研究,多为美国政府作品,可自由下载。

## 搜索

`GET /api/citations/search?q={q}&from={offset}`

- ⚠️ **接口忽略一切页大小参数,恒返回 10 条/页。**实测 `size=1` / `size=3` / `size=5` / `limit=3` / `page={"size":3}` 全部返回 10 条。因此实现**不发页大小参数**,改为按 `from` 步进 10 串行翻页,直到凑够 `-n` 或页不满为止;`-n` 越大,请求数越多(每 10 条一次)。
- `from` 是真实记录偏移,`--offset` 任意值都合法。

## 响应映射(→ Paper)

顶层 `results[]`,总数在 `stats.total`。

- `id`:`id`(整数)
- `title` / `abstract_text`:`title` / `abstract`
- `authors`:**没有 `authors` 字段**;名字在 `authorAffiliations[].meta.author.name`
- `year`:`distributionDate` 前 4 位
- `venue`:`publications[0].publicationName`
- `fields`:`subjectCategories[]`
- `pdf_url`:`downloads[].links.pdf` 是**站内相对路径**(`/api/citations/.../x.pdf`),要拼上 `https://ntrs.nasa.gov`
- `open_access`:是否有 downloads 条目
- `doi`:**恒 None**。`otherReportNumbers` 里是 NASA 报告号(如 `RTO-EN-AVT-116`),长得像标识符但**不是可解析的 DOI**,不能塞进 doi 字段
- `citations`:无

## 下载

文件名不能由 id 推出,所以先取 `/api/citations/{id}` 读记录、再跟随它给出的下载链接,见 `src/download.rs::pdf_bytes_ntrs`。

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | **无对应参数**;按 10 条/页串行翻页后本地截断 |
| `--offset` | `from`(真实记录偏移,任意值合法) |
| 其余全部 | **不支持**(未实测通过,故未声明) |
