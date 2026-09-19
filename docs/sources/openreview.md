# OpenReview

- **API 类型**: REST JSON(API v2)
- **搜索 URL**: `https://api2.openreview.net/notes/search`
- **认证**: 无需
- **能力**: 只有 search(get 与 PDF 被挡)
- **实现**: `src/sources/openreview.rs`(以代码为准)

机器学习会议(ICLR、NeurIPS …)的投稿与评审平台。相对 dblp 的独有信息是**录用结果**:拒稿、撤稿也查得到。

## 可用性(2026-09-19 实测)

- `GET /notes/search` 200。2026-09-05 实测时连它也是 403 `ChallengeRequiredError`,两周内放开过,随时可能再收紧。
- `GET /notes?id=`、`openreview.net/pdf?id=`、`openreview.net/pdf/<hash>.pdf`、`api2.openreview.net/pdf?id=` 全部 403 `ChallengeRequiredError`。

## 搜索

`GET /notes/search?term={q}&source=forum&limit={n}[&offset={k}][&group={g}]`(参数见 `api2.openreview.net/docs/api.yml`)

- **必须带 `source=forum`**:不带时 `graph neural network` 前 5 条全是 `Official_Review`。
- `group=ICLR.cc/2025/Conference` 精确到届(`diffusion` 1,335 条:录用 370、拒稿 343、撤稿 284、直接拒稿 3),`group=ICLR.cc` 是整个会议族。
- `limit=1000` 一次返回 1000 条(约 4 MB)。`sort=cdate:desc` **被忽略**(顺序与不带时逐条相同)。
- 结果混有从 dblp 导入的期刊记录(`venueid` 形如 `dblp.org/journals/NN/2024`)。

## 响应映射(→ Paper)

- `id` = `notes[].id`(forum id);`url` = `openreview.net/forum?id=<id>`
- `title` / `abstract` / `venue` / `keywords` = `content.<key>.value`
- `authors` = `content.authors.value[]`,字符串或 `{fullname}` 对象
- `year` = `pdate`,无则 `cdate`(毫秒时间戳,UTC)
- `pdf_url` = `content.pdf.value`,以 `/` 开头的补成 `https://openreview.net…`
