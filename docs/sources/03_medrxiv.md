# MedRxiv

- **API 类型**: REST JSON(与 BioRxiv 共用 API)
- **基础 URL**: `https://api.biorxiv.org`(下载 `https://www.medrxiv.org`)
- **认证**: 无需
- **能力**: search + download + read
- **实现**: `src/sources/medrxiv.rs`(以代码为准)

## 与 BioRxiv 的差异

逻辑、时间窗、本地关键词过滤、退避重试均与 `02_biorxiv.md` 完全一致,仅以下不同:

- 搜索路径:`GET {base}/details/medrxiv/{start_date}/{end_date}/0`
- 论文/PDF URL 域名:`www.medrxiv.org`
- `venue`:固定 `"medRxiv preprint"`
- `source`:`"medrxiv"`
- 日期格式化复用 `biorxiv::format_date`。
