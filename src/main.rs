mod cli;
mod identifier;
mod output;
mod sources;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::Search(_args) => {
            eprintln!("search: not yet implemented");
            std::process::exit(1);
        }
        cli::Commands::Download(_args) => {
            eprintln!("download: not yet implemented");
            std::process::exit(1);
        }
        cli::Commands::Read(_args) => {
            eprintln!("read: not yet implemented");
            std::process::exit(1);
        }
        cli::Commands::Get(_args) => {
            eprintln!("get: not yet implemented");
            std::process::exit(1);
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
