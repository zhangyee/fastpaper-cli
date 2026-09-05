# Per-Source API Research Notes

These are the research notes each source implementation was built from — endpoints, parameters, response shapes, auth, and rate-limit behavior. **They are written in Chinese**; this page is the English index. When adding a source, contribute your note here following the same format (see [../adding-a-source.md](../adding-a-source.md), Step 0).

The sources fall into a few families: open preprint servers with free full text (arXiv, bioRxiv, medRxiv), biomedical databases run by NLM/EMBL-EBI (PubMed metadata, PMC/Europe PMC full text), academic search engines that index rather than host (Semantic Scholar), metadata registries (CrossRef, OpenAlex, DBLP), and open-access aggregators/archives (CORE, OpenAIRE, DOAJ, Unpaywall, Zenodo, HAL).

[base.md](base.md) documents the shared `Paper` contract and cross-source HTTP conventions.

| Note | Source | API type | Auth | Rate-limit / quirks |
|---|---|---|---|---|
| [arxiv](arxiv.md) | arXiv | REST, Atom/XML | none | be gentle; ~1 req/3s recommended |
| [biorxiv](biorxiv.md) | bioRxiv | REST JSON | none | shares API with medRxiv |
| [medrxiv](medrxiv.md) | medRxiv | REST JSON | none | same API as bioRxiv, different collection |
| [pubmed](pubmed.md) | PubMed | E-utilities REST, XML | optional `NCBI_API_KEY` | 3 req/s → 10 req/s with key |
| [pmc](pmc.md) | PMC | E-utilities REST, XML | optional `NCBI_API_KEY` | same E-utilities limits as PubMed |
| [europepmc](europepmc.md) | Europe PMC | REST JSON | none | |
| [semantic](semantic.md) | Semantic Scholar | REST JSON (Graph v1) | optional `SEMANTIC_SCHOLAR_API_KEY` | 1 req/s anon → 100 req/s with key |
| [crossref](crossref.md) | CrossRef | REST JSON | none; `FASTPAPER_EMAIL` joins polite pool | |
| [openalex](openalex.md) | OpenAlex | REST JSON | none; `FASTPAPER_EMAIL` joins polite pool | |
| [dblp](dblp.md) | DBLP | REST XML (+ HTML fallback) | none | |
| [core](core.md) | CORE | REST JSON (v3) | optional `CORE_API_KEY` | key improves limits and result quality |
| [openaire](openaire.md) | OpenAIRE | REST JSON | none | |
| [doaj](doaj.md) | DOAJ | REST JSON | none | |
| [unpaywall](unpaywall.md) | Unpaywall | REST JSON | **required** `UNPAYWALL_EMAIL` | free; email required by API terms |
| [zenodo](zenodo.md) | Zenodo | REST JSON | none | |
| [hal](hal.md) | HAL | REST JSON (Solr) | none | |

