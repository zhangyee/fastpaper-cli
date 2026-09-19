# Hugging Face Papers

- **API 类型**: REST JSON
- **端点**: `https://huggingface.co/api/papers/search`、`/api/daily_papers`、`/api/papers/<id>`
- **认证**: 无需;`HF_TOKEN` 可提额
- **能力**: search(关键词 + `--trending` / `--top` 榜单)+ get;无 download(id 即 arXiv id)
- **实现**: `src/sources/huggingface.rs`(以代码为准)

AI 社区每天挑论文、投票的地方。价值不在覆盖(每篇都是 arXiv 论文),而在**关注度**。参数以官方 OpenAPI(`huggingface.co/.well-known/openapi.json`)为准,行为以 2026-09-19 实测为准。

## 榜单 `GET /api/daily_papers`

- `date=YYYY-MM-DD` / `week=YYYY-Www` / `month=YYYY-MM`:该时段上榜的论文,**按 upvotes 降序**。
- `sort=trending`:HF 自己的滚动热度序,**忽略日期参数**(带不带 `month=` 逐条相同,顺序也不按票数)。
- `limit` ≤ 100,`p` 是页号(页大小即 `limit`,所以跨页必须保持同一个 `limit`)。2026-08 整月不到 900 条。
- `week` 的正则只到 **W52**:W53 返回 400。周末的 `date` 返回 `[]`;晚于最新一天返回 400。

## 检索 `GET /api/papers/search?q=&limit=`

`limit` ≤ 120,不能翻页,按相关度;经典老论文排前。条目形状与榜单相同(`{paper, numComments, organization, …}`)。

## 单篇 `GET /api/papers/<arXiv id>`

论文对象本身,另有 `numTotalModels` / `numTotalDatasets` / `numTotalSpaces`(同名 `linked*` 是明细数组,不用)。

## 响应映射(→ Paper)

- `id` = `paper.id`(arXiv id);`url` = `huggingface.co/papers/<id>`;`pdf_url` = `arxiv.org/pdf/<id>`;`open_access` = true
- `year` = `publishedAt` 前 4 位;标题与摘要去掉硬换行
- `community`:`upvotes`、`numComments`(外层)、`organization.name`、`githubRepo` / `githubStars`(月榜 73/100 有)、`submittedOnDailyAt` 日期部分(实时热门里的老论文为 null)

## 限流

匿名每 IP 每 5 分钟 500 次(响应头 `ratelimit-policy: "fixed window";"api";q=500;w=300`);429 时 `ratelimit` 头的 `t=` 是距重置秒数。
