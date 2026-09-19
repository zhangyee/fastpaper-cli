# J-STAGE

- **API 类型**: REST,Atom XML(J-STAGE WebAPI,`service=3` 论文检索)
- **搜索 URL**: `https://api.jstage.jst.go.jp/searchapi/do?service=3`
- **认证**: 无需
- **能力**: search + download(无 get)
- **实现**: `src/sources/jstage.rs`(以代码为准)

日本学协会期刊(JST 运营)。依据《J-STAGE WebAPI ご利用マニュアル》Ver.2.0(2026-03-26)。

## 搜索参数(service=3)

`text`(全文)、`article`(标题)、`author`、`affil`、`keyword`、`abst`、`material`(刊名)、`issn`、`cdjournal`、`pubyearfrom` / `pubyearto`(4 位年)、`vol` / `no`、`start`、`count`(≤ 1000)、`sortflg`。同一参数内半角空格 = AND。实现用 `text` 放检索词。`sortflg=2` 必须同时给 `material` 或 `issn`,否则 `ERR_014`,所以不声明 `--sort`。

## 状态码(`<result><status>`)

`0` 正常;`WARN_002` 命中超过上限(正常);**`ERR_001` 0 件**(当空结果);`ERR_003` 并发超限(当限流重试);`ERR_004`–`ERR_014`、`SYS_ERR_009` 为参数或系统错误。

## 响应映射(→ Paper)

每个 `entry` 的 `article_title` / `article_link` / `author` / `material_title` 都有 `en` 与 `ja` 两份。**有日文标题就用日文**,作者、刊名、落地页跟随同一语言,缺哪份就用另一份。

- `id` = 文章页路径 `/article/` 与 `/_article` 之间那段(如 `faruawpsj/54/9/54_843`)
- `pdf_url` = 同一路径加 `/_pdf`(实测拉到 `%PDF`);`doi` = `prism:doi`;`year` = `pubyear` 前 4 位
- **不返回抄録**:`abstract` 恒为 null;论文级接口不给认证类型:`open_access` 恒为 null
- 怪癖:CDATA 里出现字面 `&apos;`,解析时还原
