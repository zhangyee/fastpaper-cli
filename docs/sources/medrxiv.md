# MedRxiv

- **API 类型**: REST JSON(与 BioRxiv 共用 API)
- **基础 URL**: `https://api.biorxiv.org`(下载 `https://www.medrxiv.org`)
- **认证**: 无需
- **能力**: search + get
- `get`:`GET /details/medrxiv/{doi}`
- **不提供 download**:`https://www.medrxiv.org/content/{doi}v{n}.full.pdf` 对项目 UA 和浏览器 UA **一律返回 403**,不是换 header 能解决的(bioRxiv 同架构却放行)
- **实现**: `src/sources/medrxiv.rs`(以代码为准)

## 与 BioRxiv 的差异

逻辑、时间窗、本地关键词过滤、退避重试均与 `biorxiv.md` 完全一致,仅以下不同:

- 搜索路径:`GET {base}/details/medrxiv/{start_date}/{end_date}/0`
- 论文/PDF URL 域名:`www.medrxiv.org`
- `venue`:固定 `"medRxiv preprint"`
- `source`:`"medrxiv"`
- 日期格式化复用 `biorxiv::format_date`。

## CLI 过滤参数映射

`fastpaper search` 的参数落到本源原生参数上的方式。源不支持的参数会直接报错,不会被静默忽略。

| CLI 参数 | 映射 |
|---|---|
| `--year` / `--after` / `--before` | **决定 `/details/medrxiv/{start}/{end}/{cursor}` 的日期区间**——这是本源唯一能原生支持的过滤 |
| `-n` / `--offset` | 本地切片。游标每页 30 条,最多翻 20 页 |
| `--open-access` | 恒满足 |
| `--sort` / `--author` / `--field` | 不支持 |
