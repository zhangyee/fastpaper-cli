# NASA ADS(SciX)

- **API 类型**: REST JSON(Solr)
- **搜索 URL**: `https://api.adsabs.harvard.edu/v1/search/query`
- **认证**: **必需** `ADS_API_TOKEN`(头 `Authorization: Bearer <token>`)
- **能力**: search + get + download + cite
- **实现**: `src/sources/ads.rs`(以代码为准)

天文 / 物理权威文献系统,正更名为 SciX。依据 `adsabs/adsabs-dev-api` 仓库的 Search API 说明、ADS 帮助页,以及 2026-09-19 用真 token 实测。

## token

免费:在 `scixplorer.org` 注册登录,到 `scixplorer.org/user/settings/token` 生成(旧址 `ui.adsabs.harvard.edu/user/settings/token` 仍可用)。只读 `ADS_API_TOKEN`(与 Python 库 `ads` 一致;官方 scix-mcp 用 `SCIX_API_TOKEN`,不读)。KyDog 的「文献检索密钥」预设用同一个名字。

## 搜索

`GET /search/query?q=&fl=&rows=&start=&fq=&sort=`

- `q` 原样透传 Solr 语法;`fq` 可重复;`rows` 文档上限 2000,实现上限 1000(约 4.8 KB/条,2000 条逼近 10 MB 响应上限)。
- `sort`:`citation_count desc`、`date desc`、`score desc`(实测生效)。
- `pubdate` 只到月(日恒 `00`),所以不声明 `--after` / `--before`。
- 限额:search 5000 次 / 天 / token,UTC 零点重置;响应头 `x-ratelimit-limit` / `-remaining` / `-reset`(Unix 秒)。
- 401 正文:`The access token provided is expired, revoked, malformed, or invalid for other reasons.`

## 字段(实测形状)

`title` / `author` / `doi` / `identifier` / `keyword` / `property` / `esources` 是数组;`year` 是字符串;没有值的键不出现。`doi` 数组混有 arXiv DOI(`10.48550/…`),取第一个非它的。`identifier` 里 `arXiv:<id>` 给出 arXiv 版。

## 单篇、引用边

- 单篇:`q=identifier:"<bibcode|DOI|arXiv:id>"`,三种实测都一条命中。
- 引用边:`citations(bibcode:X)`(谁引用了它)、`references(bibcode:X)`(它引用了谁);Hubble 1929 被引 1,275,EHT 参考文献 121。

## 下载(`https://ui.adsabs.harvard.edu/link_gateway/<bibcode>/<kind>`)

网关**不需要 token**。只试 `esources` 里有的:`EPRINT_PDF`(跳 arXiv)→ `ADS_PDF`(ADS 扫描的历史期刊,**现场生成**:首次 63 s 后 504,再取 2.7 s 成功,所以 504 时等 10 s 重试一次)→ `PUB_PDF`(出版商,常 403)。`PMC_PDF` 跳过:落地是 HTML 验证页。
