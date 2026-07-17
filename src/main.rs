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
    load_env();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str);
    // Tokens after the subcommand, forwarded to that command's parser.
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    // Emit a version banner (to stderr) for actual subcommands so run logs
    // record which build executed. The bare/help/version arms below print
    // the banner to stdout themselves, so skip those here to avoid dupes.
    if !matches!(
        cmd,
        None | Some("help" | "--help" | "-h" | "version" | "--version" | "-V")
    ) {
        eprintln!("pocketsmith {VERSION} (commit {GIT_COMMIT}, built {BUILD_DATE})");
    }

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

/// Load environment variables from `.env` files, most-specific first.
///
/// Precedence (a variable already set is never overridden by dotenvy):
///   1. real process environment (e.g. exported by launchd/shell)
///   2. `.env` in the cwd or a parent dir (dev convenience)
///   3. `$XDG_CONFIG_HOME/pocketsmith/env`, falling back to
///      `$HOME/.config/pocketsmith/env`
///
/// The XDG fallback lets scheduled jobs (which run from an arbitrary cwd)
/// pick up secrets like `POCKETSMITH_API_KEY` from a fixed user-config file,
/// so the key never has to be baked into the launchd job definition.
fn load_env() {
    // 2. cwd-relative .env (walks up parent dirs). Ignore if absent.
    dotenvy::dotenv().ok();

    // 3. Fixed user-config fallback.
    if let Some(path) = config_env_path() {
        dotenvy::from_path(&path).ok();
    }
}

/// Path to the user-config env file: `$XDG_CONFIG_HOME/pocketsmith/env`,
/// falling back to `$HOME/.config/pocketsmith/env`. Returns `None` only when
/// neither `XDG_CONFIG_HOME` nor `HOME` is set.
fn config_env_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| std::path::PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("pocketsmith").join("env"))
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
