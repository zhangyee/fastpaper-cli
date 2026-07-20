# MedRxiv

- **API类型**: REST JSON (与 BioRxiv 共用 API)
- **基础URL**: `https://api.biorxiv.org/details/medrxiv`
- **认证**: 无需
- **能力**: search + download + read
- **限制**: 同 BioRxiv，API 按类别+日期范围浏览，不支持关键词全文搜索。

## 逻辑与 BioRxiv 完全一致

唯一区别:
- BASE_URL 路径中 `biorxiv` → `medrxiv`
- 论文 URL 域名: `www.medrxiv.org` 而非 `www.biorxiv.org`
- `source: "medrxiv"`
- `venue: Some("medRxiv preprint")`

其余 search / download / read 逻辑与 `02_biorxiv.md` 相同，参见该文件。
