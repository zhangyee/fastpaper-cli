use clap::Parser;

use fastpaper::cli;
use fastpaper::commands::{self, CommandError, CommandResult};

fn main() {
    let cli = cli::Cli::parse();

    let result: CommandResult = match &cli.command {
        cli::Commands::Search(args) => commands::search::run(args, &cli.global),
        cli::Commands::Get(args) => commands::get::run(args, &cli.global),
        cli::Commands::Download(args) => commands::download::run(args, &cli.global),
        cli::Commands::Cite(args) => commands::cite::run(args, &cli.global),
        cli::Commands::Read(args) => commands::read::run(args, &cli.global),
        cli::Commands::Sources(args) => commands::sources::run(args),
        cli::Commands::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut <cli::Cli as clap::CommandFactory>::command(),
                "fastpaper",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    };

    if let Err(err) = result {
        // AlreadyExists exits 0: the file being there already is not a failure.
        let prefix = match err {
            CommandError::AlreadyExists(_) => "",
            _ => "Error: ",
        };
        eprintln!("{}{}", prefix, err.message());
        std::process::exit(err.exit_code());
    }
}
