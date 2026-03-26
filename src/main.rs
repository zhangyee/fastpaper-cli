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
            eprintln!("sources: not yet implemented");
            std::process::exit(1);
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
