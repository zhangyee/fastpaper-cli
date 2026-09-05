# INSPIRE-HEP

- **API 类型**: REST JSON
- **搜索 URL**: `https://inspirehep.net/api/literature`
- **认证**: 无需
- **能力**: search + get(无 download)
- **实现**: `src/sources/inspire.rs`(以代码为准)

高能物理文献库(CERN / DESY / Fermilab / SLAC 合办)。与 arXiv 重叠极高,增量在**可靠的引用数**、作者消歧,以及预印本↔期刊版的关联。

## 搜索

`GET /api/literature?q={q}&size={n}&page={p}&fields={字段列表}`

- 结构化限定走 INSPIRE 自己的查询语言拼进 `q`,没有独立参数:作者 `a Ellis, John`,年份 `de 2015`,多条件用 ` and ` 连接。
- `fields` 显式列出要返回的字段,实现请求:`titles,authors,abstracts,dois,publication_info,citation_count,arxiv_eprints,earliest_date`。
- 排序:`sort=mostrecent`(日期)、`sort=mostcited`(引用数)——**两者实测均生效**。
- 翻页是页码,`--offset` 必须是 `-n` 的整数倍。

## 响应映射(→ Paper)

顶层 `hits.hits[]`,每条的元数据在 `.metadata`。

- `id`:`hits.hits[].id`(即 control number)
- `title`:`metadata.titles[0].title`
- `authors`:`metadata.authors[].full_name`
- `abstract_text`:`metadata.abstracts[0].value`
- `doi`:`metadata.dois[0].value`
- `year`:`metadata.publication_info[0].year`,回退 `metadata.earliest_date` 前 4 位
- `venue`:`metadata.publication_info[0].journal_title`
- `citations`:`metadata.citation_count`
- `pdf_url`:由 `metadata.arxiv_eprints[0].value` 拼成 `https://arxiv.org/pdf/{eprint}`——**文件在 arXiv,不在 INSPIRE**
- `open_access`:无

## 注意

- `documents` 字段实测多为 null,INSPIRE 自己不托管 PDF,所以 `download` 能力为 false;要文件用 `fastpaper download <arxiv id>`。
- `get` 支持纯数字 control number 走 `/api/literature/{id}`;DOI / arXiv id 走 `q=doi ...` / `q=arxiv ...`。

## CLI 过滤参数映射

| CLI 参数 | 映射 |
|---|---|
| `-n` | `size` |
| `--offset` | `page` 页码;必须是 `-n` 的整数倍 |
| `--author` | `q` 内 ` and a {name}` |
| `--year` | `q` 内 ` and de {year}` |
| `--sort` | `sort=mostrecent`(date)/ `sort=mostcited`(citations) |
| `--after` / `--before` / `--field` / `--open-access` | **不支持** |
