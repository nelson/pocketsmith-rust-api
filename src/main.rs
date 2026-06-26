//! `pocketsmith` — unified CLI for the PocketSmith sync/normalise/transfer
//! toolkit. The first argument selects a subcommand; everything after it is
//! forwarded to that command's own argument parser.
//!
//!   pocketsmith sync                 pull PocketSmith data into the local DB
//!   pocketsmith transfers [...]      detect/apply/annotate transfer pairs
//!   pocketsmith normalise [...]      scan/apply payee normalisations
//!   pocketsmith push [...]           push confirmed changes to PocketSmith
//!   pocketsmith dump                 export rule tables to rules/*.sql
//!   pocketsmith rule <verb> [...]    view/edit normalisation rules
//!   pocketsmith serve                run the local web UI (web feature)
//!   pocketsmith version              print the current release version
//!   pocketsmith help                 print version + usage

use std::process::ExitCode;

mod cli;
mod rule;
#[cfg(feature = "web")]
mod serve;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_COMMIT: &str = env!("GIT_COMMIT");
const BUILD_DATE: &str = env!("BUILD_DATE");

fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str);
    // Tokens after the subcommand, forwarded to that command's parser.
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    match cmd {
        // No command: show the version banner and usage (exit 0).
        None => {
            print_version();
            print_help();
            ExitCode::SUCCESS
        }
        Some("help" | "--help" | "-h") => {
            print_version();
            print_help();
            ExitCode::SUCCESS
        }
        Some("version" | "--version" | "-V") => {
            print_version();
            ExitCode::SUCCESS
        }
        Some("sync") => to_code(cli::sync::run(rest)),
        Some("transfers") => to_code(cli::transfers::run(rest)),
        Some("normalise") => to_code(cli::normalise::run(rest)),
        Some("push") => cli::push::run(rest),
        Some("dump") => to_code(cli::dump::run(rest)),
        Some("rule") => rule::run_main(rest),
        #[cfg(feature = "web")]
        Some("serve") => to_code(serve::run(rest)),
        #[cfg(not(feature = "web"))]
        Some("serve") => {
            eprintln!(
                "error: `serve` was not compiled in. Rebuild with the `web` feature:\n  \
                 cargo build --release --features web"
            );
            ExitCode::from(2)
        }
        Some(other) => {
            eprintln!("error: unknown command {other:?}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

/// Map an `anyhow::Result<()>` command outcome to a process exit code,
/// printing the error chain on failure.
fn to_code(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_version() {
    println!("pocketsmith {VERSION} (commit {GIT_COMMIT}, built {BUILD_DATE})");
}

fn print_help() {
    println!(
        "\nUSAGE\n  \
         pocketsmith <command> [options]\n\n\
         COMMANDS\n  \
         sync               Pull PocketSmith data into the local SQLite mirror\n  \
         transfers [...]    Detect / apply / annotate transfer pairs\n  \
         normalise [...]    Scan / apply payee normalisations\n  \
         push [...]         Push confirmed local changes to PocketSmith\n  \
         dump               Export the live rule tables to rules/*.sql\n  \
         rule <verb> [...]  View / edit normalisation rules (try `rule --help`)\n  \
         serve              Run the local web review UI (requires the `web` feature)\n  \
         version            Print the current release version\n  \
         help               Print this help\n\n\
         The database path defaults to $XDG_DATA_HOME/pocketsmith/pocketsmith.db\n  \
         (override with POCKETSMITH_DB)."
    );
}
