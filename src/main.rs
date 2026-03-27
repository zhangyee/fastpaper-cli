mod cli;
mod download;
mod identifier;
mod output;
mod read;
mod sources;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::Search(args) => {
            if !args.source.supports_search() {
                eprintln!(
                    "Error: source '{}' does not support 'search'.",
                    args.source.name()
                );
                std::process::exit(1);
            }
            let result = match args.source {
                cli::Source::Arxiv => {
                    let base_url = std::env::var("FASTPAPER_ARXIV_URL")
                        .unwrap_or_else(|_| "https://export.arxiv.org".to_string());
                    sources::arxiv::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Semantic => {
                    let base_url = std::env::var("FASTPAPER_SEMANTIC_URL")
                        .unwrap_or_else(|_| "https://api.semanticscholar.org".to_string());
                    sources::semantic::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Crossref => {
                    let base_url = std::env::var("FASTPAPER_CROSSREF_URL")
                        .unwrap_or_else(|_| "https://api.crossref.org".to_string());
                    sources::crossref::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Openalex => {
                    let base_url = std::env::var("FASTPAPER_OPENALEX_URL")
                        .unwrap_or_else(|_| "https://api.openalex.org".to_string());
                    sources::openalex::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Pubmed => {
                    let base_url = std::env::var("FASTPAPER_PUBMED_URL")
                        .unwrap_or_else(|_| "https://eutils.ncbi.nlm.nih.gov".to_string());
                    sources::pubmed::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Pmc => {
                    let base_url = std::env::var("FASTPAPER_PMC_URL")
                        .unwrap_or_else(|_| "https://eutils.ncbi.nlm.nih.gov".to_string());
                    sources::pmc::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Europepmc => {
                    let base_url = std::env::var("FASTPAPER_EUROPEPMC_URL")
                        .unwrap_or_else(|_| "https://www.ebi.ac.uk".to_string());
                    sources::europepmc::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Dblp => {
                    let base_url = std::env::var("FASTPAPER_DBLP_URL")
                        .unwrap_or_else(|_| "https://dblp.org".to_string());
                    sources::dblp::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Core => {
                    let base_url = std::env::var("FASTPAPER_CORE_URL")
                        .unwrap_or_else(|_| "https://api.core.ac.uk".to_string());
                    sources::core::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Openaire => {
                    let base_url = std::env::var("FASTPAPER_OPENAIRE_URL")
                        .unwrap_or_else(|_| "https://api.openaire.eu".to_string());
                    sources::openaire::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Doaj => {
                    let base_url = std::env::var("FASTPAPER_DOAJ_URL")
                        .unwrap_or_else(|_| "https://doaj.org".to_string());
                    sources::doaj::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Zenodo => {
                    let base_url = std::env::var("FASTPAPER_ZENODO_URL")
                        .unwrap_or_else(|_| "https://zenodo.org".to_string());
                    sources::zenodo::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Hal => {
                    let base_url = std::env::var("FASTPAPER_HAL_URL")
                        .unwrap_or_else(|_| "https://api.archives-ouvertes.fr".to_string());
                    sources::hal::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Biorxiv => {
                    let base_url = std::env::var("FASTPAPER_BIORXIV_URL")
                        .unwrap_or_else(|_| "https://api.biorxiv.org".to_string());
                    sources::biorxiv::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Medrxiv => {
                    let base_url = std::env::var("FASTPAPER_MEDRXIV_URL")
                        .unwrap_or_else(|_| "https://api.biorxiv.org".to_string());
                    sources::medrxiv::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Scholar => {
                    let base_url = std::env::var("FASTPAPER_SCHOLAR_URL")
                        .unwrap_or_else(|_| "https://scholar.google.com".to_string());
                    sources::scholar::search(&base_url, &args.query, args.limit)
                }
                cli::Source::Unpaywall => {
                    let base_url = std::env::var("FASTPAPER_UNPAYWALL_URL")
                        .unwrap_or_else(|_| "https://api.unpaywall.org".to_string());
                    sources::unpaywall::lookup_doi(&base_url, &args.query)
                        .map(|paper| vec![paper])
                }
                cli::Source::Local => {
                    eprintln!("Error: 'local' source does not support search.");
                    std::process::exit(1);
                }
            };
            match result {
                Ok(papers) => {
                    let out = match cli.global.format {
                        cli::OutputFormat::Json => output::to_json(&papers),
                        cli::OutputFormat::Csv => output::to_csv(&papers),
                        cli::OutputFormat::Bibtex => output::to_bibtex(&papers),
                        _ => output::to_table(&papers),
                    };
                    print!("{}", out);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        cli::Commands::Download(args) => {
            if !args.source.supports_download() {
                eprintln!(
                    "Error: source '{}' does not support 'download'.",
                    args.source.name()
                );
                if let Some(hint) = args.source.download_hint() {
                    eprintln!("Hint: {}", hint);
                }
                std::process::exit(1);
            }
            match args.source {
                cli::Source::Arxiv => {
                    let base_url = std::env::var("FASTPAPER_ARXIV_URL")
                        .unwrap_or_else(|_| "https://arxiv.org".to_string());
                    match download::download_arxiv(
                        &base_url,
                        &args.identifier,
                        &args.dir,
                        args.overwrite,
                    ) {
                        Ok(path) => {
                            eprintln!("Saved: {}", path.display());
                        }
                        Err(e) if e.contains("already exists") => {
                            eprintln!("{}", e);
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    eprintln!("download: source '{}' not yet implemented", args.source.name());
                    std::process::exit(1);
                }
            }
        }
        cli::Commands::Read(args) => {
            if !args.source.supports_read() {
                eprintln!(
                    "Error: source '{}' does not support 'read'.",
                    args.source.name()
                );
                std::process::exit(1);
            }
            if args.metadata_only {
                match args.source {
                    cli::Source::Arxiv => {
                        let base_url = std::env::var("FASTPAPER_ARXIV_URL")
                            .unwrap_or_else(|_| "https://export.arxiv.org".to_string());
                        match sources::arxiv::get_by_id(&base_url, &args.identifier) {
                            Ok(Some(paper)) => {
                                let out = match cli.global.format {
                                    cli::OutputFormat::Json => output::to_json(&[paper]),
                                    cli::OutputFormat::Csv => output::to_csv(&[paper]),
                                    cli::OutputFormat::Bibtex => output::to_bibtex(&[paper]),
                                    _ => output::to_table(&[paper]),
                                };
                                print!("{}", out);
                            }
                            Ok(None) => {
                                eprintln!("Paper not found: {}", args.identifier);
                                std::process::exit(4);
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    _ => {
                        eprintln!("read: source '{}' not yet implemented", args.source.name());
                        std::process::exit(1);
                    }
                }
            } else {
                // Full text mode
                match args.source {
                    cli::Source::Local => {
                        let path = std::path::Path::new(&args.identifier);
                        match read::extract_text(path) {
                            Ok(full_text) => {
                                let text = match args.section {
                                    cli::Section::Abstract => {
                                        read::extract_section_abstract(&full_text)
                                            .unwrap_or_default()
                                    }
                                    _ => full_text.clone(),
                                };
                                let text = if let Some(max) = args.max_length {
                                    text.chars().take(max).collect::<String>()
                                } else {
                                    text
                                };
                                match cli.global.format {
                                    cli::OutputFormat::Json => {
                                        let out = serde_json::json!({
                                            "content": {
                                                "full_text": text,
                                            }
                                        });
                                        let json_str = serde_json::to_string_pretty(&out).unwrap();
                                        if let Some(ref out_path) = args.output {
                                            std::fs::write(out_path, &json_str).unwrap_or_else(|e| {
                                                eprintln!("Error writing file: {}", e);
                                                std::process::exit(1);
                                            });
                                        } else {
                                            print!("{}", json_str);
                                        }
                                    }
                                    _ => {
                                        if let Some(ref out_path) = args.output {
                                            std::fs::write(out_path, &text).unwrap_or_else(|e| {
                                                eprintln!("Error writing file: {}", e);
                                                std::process::exit(1);
                                            });
                                        } else {
                                            print!("{}", text);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    _ => {
                        eprintln!("read: full text for source '{}' not yet implemented", args.source.name());
                        std::process::exit(1);
                    }
                }
            }
        }
        cli::Commands::Get(args) => {
            let id_type = identifier::detect_id_type(&args.identifier);
            let result: Result<Option<sources::Paper>, String> = match id_type {
                identifier::IdType::Arxiv | identifier::IdType::ArxivOld => {
                    let base_url = std::env::var("FASTPAPER_ARXIV_URL")
                        .unwrap_or_else(|_| "https://export.arxiv.org".to_string());
                    sources::arxiv::get_by_id(&base_url, &args.identifier)
                }
                identifier::IdType::Doi => {
                    let base_url = std::env::var("FASTPAPER_CROSSREF_URL")
                        .unwrap_or_else(|_| "https://api.crossref.org".to_string());
                    sources::crossref::get_by_doi(&base_url, &args.identifier)
                }
                identifier::IdType::Pmc => {
                    let base_url = std::env::var("FASTPAPER_PMC_URL")
                        .unwrap_or_else(|_| "https://eutils.ncbi.nlm.nih.gov".to_string());
                    sources::pmc::get_by_pmc_id(&base_url, &args.identifier)
                }
                identifier::IdType::Pmid => {
                    let base_url = std::env::var("FASTPAPER_PUBMED_URL")
                        .unwrap_or_else(|_| "https://eutils.ncbi.nlm.nih.gov".to_string());
                    let pmid = args.identifier.strip_prefix("PMID:").unwrap_or(&args.identifier);
                    sources::pubmed::get_by_pmid(&base_url, pmid)
                }
                identifier::IdType::S2 => {
                    let base_url = std::env::var("FASTPAPER_SEMANTIC_URL")
                        .unwrap_or_else(|_| "https://api.semanticscholar.org".to_string());
                    let s2_id = args.identifier.strip_prefix("S2:").unwrap_or(&args.identifier);
                    sources::semantic::get_by_id(&base_url, s2_id)
                }
                identifier::IdType::Url => {
                    eprintln!("URL-based lookup not yet implemented");
                    std::process::exit(1);
                }
                identifier::IdType::Unknown => {
                    eprintln!(
                        "Unrecognized identifier format: '{}'\nSupported formats: arXiv ID, DOI, PMC ID, PMID, S2 ID, URL",
                        args.identifier
                    );
                    std::process::exit(1);
                }
            };
            match result {
                Ok(Some(paper)) => {
                    let out = match cli.global.format {
                        cli::OutputFormat::Json => output::to_json(&[paper]),
                        cli::OutputFormat::Csv => output::to_csv(&[paper]),
                        cli::OutputFormat::Bibtex => output::to_bibtex(&[paper]),
                        _ => output::to_table(&[paper]),
                    };
                    print!("{}", out);
                }
                Ok(None) => {
                    eprintln!("Paper not found: {}", args.identifier);
                    std::process::exit(4);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        cli::Commands::Sources(_args) => {
            println!("Source          search  download  read");
            println!("──────────────────────────────────────");
            println!("arxiv          ✓       ✓         ✓");
            println!("biorxiv        ✓       ✓         ✓");
            println!("medrxiv        ✓       ✓         ✓");
            println!("pubmed         ✓       ✗         ✗");
            println!("pmc            ✓       ✓         ✓");
            println!("europepmc      ✓       ✓         ✓");
            println!("scholar        ✓       ✗         ✗");
            println!("semantic       ✓       ✓         ✓");
            println!("crossref       ✓       ✗         ✗");
            println!("openalex       ✓       ✗         ✗");
            println!("dblp           ✓       ✗         ✗");
            println!("core           ✓       ✓         ✓");
            println!("openaire       ✓       ✗         ✗");
            println!("doaj           ✓       ✓         ✓");
            println!("unpaywall      ✓       ✗         ✗");
            println!("zenodo         ✓       ✓         ✓");
            println!("hal            ✓       ✓         ✓");
        }
        cli::Commands::Skill { action } => {
            const SKILL_CONTENT: &str = include_str!("../skills/fastpaper/SKILL.md");
            match action {
                cli::SkillAction::Show => {
                    print!("{}", SKILL_CONTENT);
                }
                cli::SkillAction::Export { agent } => {
                    if let Some(target) = agent {
                        let path = match target {
                            cli::AgentTarget::Claude => "~/.claude/skills/fastpaper/SKILL.md",
                            cli::AgentTarget::Codex => ".codex/skills/fastpaper/SKILL.md",
                            cli::AgentTarget::Cursor => ".cursor/skills/fastpaper/SKILL.md",
                            cli::AgentTarget::Gemini => ".gemini/skills/fastpaper/SKILL.md",
                        };
                        eprintln!("Install to: {}", path);
                    }
                    print!("{}", SKILL_CONTENT);
                }
                cli::SkillAction::Install { agent: _ } => {
                    eprintln!("skill install: not yet implemented");
                    std::process::exit(1);
                }
            }
        }
        cli::Commands::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut <cli::Cli as clap::CommandFactory>::command(),
                "fastpaper",
                &mut std::io::stdout(),
            );
        }
    }
}
