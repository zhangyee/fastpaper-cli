mod cli;
mod identifier;
mod output;
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
            eprintln!("search: not yet implemented");
            std::process::exit(1);
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
            eprintln!("download: not yet implemented");
            std::process::exit(1);
        }
        cli::Commands::Read(args) => {
            if !args.source.supports_read() {
                eprintln!(
                    "Error: source '{}' does not support 'read'.",
                    args.source.name()
                );
                std::process::exit(1);
            }
            eprintln!("read: not yet implemented");
            std::process::exit(1);
        }
        cli::Commands::Get(args) => {
            let id_type = identifier::detect_id_type(&args.identifier);
            match id_type {
                identifier::IdType::Arxiv | identifier::IdType::ArxivOld => {
                    // TODO: call arxiv get by id
                    eprintln!("arXiv source not yet implemented for get");
                    std::process::exit(1);
                }
                identifier::IdType::Doi => {
                    match sources::crossref::get_by_doi(
                        "https://api.crossref.org",
                        &args.identifier,
                    ) {
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
                            std::process::exit(3);
                        }
                    }
                }
                identifier::IdType::Pmc => {
                    eprintln!("pmc source not yet implemented");
                    std::process::exit(1);
                }
                identifier::IdType::Pmid => {
                    eprintln!("pubmed source not yet implemented");
                    std::process::exit(1);
                }
                identifier::IdType::S2 => {
                    eprintln!("semantic scholar source not yet implemented for get");
                    std::process::exit(1);
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
            }
        }
        cli::Commands::Sources(_args) => {
            println!("Source          search  download  read");
            println!("──────────────────────────────────────");
            println!("arxiv          ✓       ✓         ✓");
            println!("biorxiv        ✓       ✓         ✓");
            println!("medrxiv        ✓       ✓         ✓");
            println!("ssrn           ✓       ✓         ✓");
            println!("pubmed         ✓       ✗         ✗");
            println!("pmc            ✓       ✓         ✓");
            println!("europepmc      ✓       ✓         ✓");
            println!("scholar        ✓       ✗         ✗");
            println!("semantic       ✓       ✓         ✓");
            println!("base           ✓       ✓         ✓");
            println!("citeseerx      ✓       ✓         ✓");
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
        cli::Commands::Skill { action: _ } => {
            eprintln!("skill: not yet implemented");
            std::process::exit(1);
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
