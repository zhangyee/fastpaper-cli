# CrossRef

- **API 类型**: REST JSON
- **基础 URL**: `https://api.crossref.org`
- **认证**: 无需;`mailto` 参数进入 polite pool,取 `FASTPAPER_EMAIL`(fallback 硬编码项目邮箱)
- **能力**: search + get_by_doi(仅元数据,不支持 download / read)
- **实现**: `src/sources/crossref.rs`(以代码为准)

## 搜索

- `GET /works`,参数:`query`、`rows`(≤1000)、`offset`、`mailto`。
- 排序:`sort` ∈ `relevance` | `published` | `is-referenced-by-count`,`order` ∈ `asc` | `desc`。
- 过滤:`filter` 参数逗号分隔,如 `from-pub-date:YYYY-MM-DD`、`until-pub-date:YYYY-MM-DD`、`has-full-text:true`。

## 单篇查询(DOI)

- `GET /works/{doi}?mailto=…`,404 → `Ok(None)`;`fastpaper get` 对 DOI 类标识符路由到这里(CrossRef 独有能力)。

## 响应映射(message.items[] → Paper)

- `id`/`doi` ← `DOI`;`title` ← `title[0]`(title 是数组);`authors` ← `author[]` 的 `given + family`
- `year`:依次尝试 `published` → `issued` → `created` 的 `date-parts[0][0]`
- `url` ← `URL`,缺失时拼 `https://doi.org/{doi}`;`venue` ← `container-title[0]`
- `citations` ← `is-referenced-by-count`;`abstract` ← `abstract`
- `pdf_url` 恒 `None`;`open_access` 恒 `None`(CrossRef 不提供 OA 状态)
- 总数在 `message.total-results`。

## 注意

- 响应中 `link[]`(content-type 含 pdf)与 `resource.primary.URL` 可能含 PDF 链接,当前实现未提取。
- 元数据源:下载/阅读需把 DOI 交给其他源(如 `fastpaper download semantic <DOI>`)。
