# DBLP

- **API 类型**: REST XML
- **基础 URL**: `https://dblp.org`,搜索端点 `GET /search/publ/api`
- **认证**: 无需
- **能力**: search only(CS 专属;无摘要;不支持 download / read)
- **实现**: `src/sources/dblp.rs`(以代码为准)

## 搜索

- 参数:`q`、`format=xml`、`h`(条数,≤1000)、`f`(偏移)。
- 查询语法(过滤条件直接拼进 `q`):`year:2020`、年份范围 `year:2020:2023`、`author:{name}`、`venue:{venue}`。
- 不支持排序、不支持 OA 过滤。

## 响应映射(XML hit/info → Paper)

- `id` ← `<key>`(DBLP key);`title` ← `<title>`;`authors` ← `<authors><author>`
- `year` ← `<year>`;`venue` ← `<venue>`;`url` ← `<url>`(dblp 落地页)
- `doi`:优先 `<doi>` 元素;否则从 `<ee>` 中含 `doi.org/` 的链接截取后半段
- 恒缺:`abstract_text`、`pdf_url`、`citations` 为 None,`fields` 为空,`open_access` 为 None

## 注意

- 设计期考虑过 HTML 爬取降级(`/search/publ` 页面),未实现;当前只有 XML API。
- 元数据源:拿 DOI 去其他源下载。
