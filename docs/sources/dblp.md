# DBLP

- **API 类型**: SPARQL(QLever),结果为 `application/sparql-results+json`
- **基础 URL**: `https://sparql.dblp.org`,端点 `GET /sparql?query=...`
- **认证**: 无需;服务方说明"有限流,防激进脚本"(见 [dblp 博客](https://blog.dblp.org/2024/09/09/introducing-our-public-sparql-query-service/))
- **能力**: search
- **实现**: `src/sources/dblp.rs`(以代码为准)

## 为什么不用 dblp.org 的搜索 API

2026-09(KyDog 会话里最早 09-15 已出现)起,dblp.org 整站前面加了 Anubis 工作量证明挑战:`/search/publ/api`、`/search/author/api`、`/rec/*.xml`,连 FAQ 页面都返回 **HTTP 200 + `text/html`**,标题 "Making sure you're not a bot!"。`dblp.uni-trier.de`、`dblp.dagstuhl.de` 两个镜像同样被拦(后者还会 429);换 UA 无效。旧实现把这个 HTML 交给 XML 解析器,报 `ill-formed document: expected '</meta>'`。过挑战要在浏览器里跑它的 JS,CLI 不做。

`sparql.dblp.org` 是 dblp 官方的 SPARQL 服务,不在挑战后面。

## 查询

- **只按标题词检索**,用 QLever 文本索引:`?t ql:contains-entity ?title . ?t ql:contains-word "w1* w2*"`,多个词须同时出现。
- 与 dblp 自己的搜索一致:每个词按**前缀**匹配(`network` 命中 `Networks`),词尾 `$` 表示整词。例外:1–2 个字母的词按整词匹配 —— 实测 `a* b*` 要 7 秒。用户显式写的 `*` 照办。
- query 按非字母数字切词、转小写(索引不分大小写、按标点切分),所以 `KV-Cache` = `kv cache`。
- 旧 API 的 `year:` `author:` `venue:` `type:` `stream:` 写在 query 里会直接报错,提示改用 `--year` / `--author`;标题里自带的冒号(`BERT: pre-training`)不受影响。
- **排序**:索引自带的 `textSearch:score` 是词频,重复词越多分越高("…All You Need But You Don't Need All…" 得 5 分),不能当相关度。改为 `ORDER BY STRLEN(?title) DESC(?year)`:含全部检索词的前提下,标题越短越靠前,同长按年份新到旧。
- 内层子查询选出并分页(`LIMIT/OFFSET`),外层再取 venue、DOI、作者;**外层必须再 ORDER BY 一次**,join 后子查询的顺序不保留。
- **只检索有署名的记录**(内层要求 `?pub dblp:hasSignature ?any`)。作者 join 若写成 `OPTIONAL`,服务端要 ~2 s;拆成多个 `OPTIONAL` 虽快(55 ms)但**会错**:无署名记录的 `?s` 未绑定,会把别的论文的作者配上来(实测一篇无署名传记被配上 Marcello Pelillo 等人)。无署名记录约占 0.8%(68,231 / 8,770,346),多是卷首材料、访谈、传记。
- 服务端耗时实测 40–400 ms;总耗时 2–3 s,主要是网络往返。

## 用到的谓词(`dblp:` = `https://dblp.org/rdf/schema#`)

- `dblp:title`(标题字面量,带末尾句点)· `dblp:yearOfPublication`(`xsd:gYear`,过滤要写 `"2023"^^xsd:gYear`,与整数比较不命中)
- `dblp:publishedIn`(venue 字符串,如 `NIPS` `CoRR` `ACL (1)`)· `dblp:doi`(IRI `https://doi.org/...`)
- `dblp:hasSignature` → `dblp:signatureOrdinal`(作者序)、`dblp:signatureDblpName`(带同名编号,如 `Jian Sun 0001`)

## 响应映射(bindings → Paper)

- 每行是"一条记录 × 一位作者",按 `?pub` 折叠,保持查询给出的顺序
- `id` ← `?pub` 去掉 `https://dblp.org/rec/`(即 dblp key,如 `conf/cvpr/HeZRS16`);`url` ← `?pub`(记录页)
- `title`、`year`、`venue` 直取;`doi` 去掉 `https://doi.org/` 前缀
- `authors` 按 ordinal 排序,去掉末尾 4 位同名编号(`Xiangyu Zhang 0005` → `Xiangyu Zhang`),与其他源一致
- 恒缺:`abstract_text`、`pdf_url`、`citations` 为 None,`fields` 为空,`open_access` 为 None

## 错误处理

- 429:退避 1 s、2 s 重试,共 3 次
- 非 2xx:带出 QLever 返回 JSON 里的 `exception`
- 返回 HTML(`text/html` 或正文以 `<` 开头):直接报"SPARQL 服务返回了 HTML 页(反爬或停机页)",不交给 JSON 解析器

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `-n` | 内层 `LIMIT`(≤1000) |
| `--offset` | 内层 `OFFSET`(≤999970) |
| `--year` | `?pub dblp:yearOfPublication "YYYY"^^xsd:gYear` |
| `--author` | 任一署名的 `signatureDblpName` 小写后包含该字符串(`CONTAINS(LCASE(...))`) |
| 其余全部 | 不支持(dblp 只有年份,`--after/--before` 会失真,故不开) |
