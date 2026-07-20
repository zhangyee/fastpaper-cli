# Per-Source API Research Notes

These are the research notes each source implementation was built from — endpoints, parameters, response shapes, auth, and rate-limit behavior. **They are written in Chinese**; this page is the English index. When adding a source, contribute your note here following the same format (see [../adding-a-source.md](../adding-a-source.md), Step 0).

The sources fall into a few families: open preprint servers with free full text (arXiv, bioRxiv, medRxiv), biomedical databases run by NLM/EMBL-EBI (PubMed metadata, PMC/Europe PMC full text), academic search engines that index rather than host (Google Scholar, Semantic Scholar, Baidu Xueshu), metadata registries (CrossRef, OpenAlex, DBLP), and open-access aggregators/archives (CORE, OpenAIRE, DOAJ, Unpaywall, Zenodo, HAL).

[00_base.md](00_base.md) documents the shared `Paper` contract and cross-source HTTP conventions (kept in sync with the code).

| Note | Source | API type | Auth | Rate-limit / quirks |
|---|---|---|---|---|
| [01](01_arxiv.md) | arXiv | REST, Atom/XML | none | be gentle; ~1 req/3s recommended |
| [02](02_biorxiv.md) | bioRxiv | REST JSON | none | shares API with medRxiv |
| [03](03_medrxiv.md) | medRxiv | REST JSON | none | same API as bioRxiv, different collection |
| [04](04_pubmed.md) | PubMed | E-utilities REST, XML | optional `NCBI_API_KEY` | 3 req/s → 10 req/s with key |
| [05](05_pmc.md) | PMC | E-utilities REST, XML | optional `NCBI_API_KEY` | same E-utilities limits as PubMed |
| [06](06_europepmc.md) | Europe PMC | REST JSON | none | |
| [07](07_google_scholar.md) | Google Scholar | HTML scraping (no official API) | none | aggressive anti-bot; experimental |
| [08](08_semantic.md) | Semantic Scholar | REST JSON (Graph v1) | optional `SEMANTIC_SCHOLAR_API_KEY` | 1 req/s anon → 100 req/s with key |
| [09](09_crossref.md) | CrossRef | REST JSON | none; `FASTPAPER_EMAIL` joins polite pool | |
| [10](10_openalex.md) | OpenAlex | REST JSON | none; `FASTPAPER_EMAIL` joins polite pool | |
| [11](11_dblp.md) | DBLP | REST XML (+ HTML fallback) | none | |
| [12](12_core.md) | CORE | REST JSON (v3) | optional `CORE_API_KEY` | key improves limits and result quality |
| [13](13_openaire.md) | OpenAIRE | REST JSON | none | |
| [14](14_doaj.md) | DOAJ | REST JSON | none | |
| [15](15_unpaywall.md) | Unpaywall | REST JSON | **required** `UNPAYWALL_EMAIL` | free; email required by API terms |
| [16](16_zenodo.md) | Zenodo | REST JSON | none | |
| [17](17_hal.md) | HAL | REST JSON (Solr) | none | |
| [18](18_xueshu.md) | Baidu Xueshu | undocumented JSON API | none (static `Acs-Token` fallback) | captcha-based risk control; experimental |

