use clap::Parser;
use clap::error::{ContextKind, ContextValue, ErrorKind};

use fastpaper::cli;
use fastpaper::commands::{self, CommandError, CommandResult};
use fastpaper::registry;

fn main() {
    let cli = cli::Cli::try_parse().unwrap_or_else(|err| exit_on_parse_error(err));

    let result: CommandResult = match &cli.command {
        cli::Commands::Search(args) => commands::search::run(args, &cli.global),
        cli::Commands::Get(args) => commands::get::run(args, &cli.global),
        cli::Commands::Download(args) => commands::download::run(args),
        cli::Commands::Cite(args) => commands::cite::run(args, &cli.global),
        cli::Commands::Read(args) => commands::read::run(args, &cli.global),
        cli::Commands::Figures(args) => commands::figures::run(args),
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

/// Let clap report a bad command line, adding the way on when the rejected
/// value names a source that was removed. clap's list of valid names says what
/// exists now, not where the caller's search went.
fn exit_on_parse_error(err: clap::Error) -> ! {
    if err.kind() == ErrorKind::InvalidValue
        && let Some(ContextValue::String(value)) = err.get(ContextKind::InvalidValue)
        && let Some(hint) = registry::retired(value)
    {
        eprintln!("error: {}", hint);
        std::process::exit(err.exit_code());
    }
    err.exit()
}
