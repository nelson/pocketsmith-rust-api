//! `rule` — view and edit normalisation rules (rule-cli §2, §4–8).
//!
//! A thin presentation shell over the `rules` library: parse → build a
//! `Mutation` → `validate_draft` → `compute_buckets` (always) → `commit`
//! (on `--apply`) → render as text or JSON. All rule semantics live in
//! the library; this binary owns only argument parsing, colour, and the
//! text/JSON rendering (rule-cli §1.1).
//!
//! Module map (kept small per file, rule-cli §14 review):
//!   args      — hand-rolled flag parsing (`Flags`)
//!   colours   — ANSI styling + regex syntax highlighting (`Style`)
//!   table     — aligned table renderer (`Cell` / `render_table`)
//!   helpers   — db open, number/text formatting, `build_rule_data`
//!   commands  — one `cmd_*` per verb (thin orchestration)
//!   render    — human (text) output
//!   json      — machine (`--json`) output

use std::process::ExitCode;

use serde_json::json;

use pocketsmith_sync::rules::model::RuleError;

mod args;
mod colours;
mod commands;
mod helpers;
mod json;
mod render;
mod table;

use args::Flags;

// ---------------------------------------------------------------------------
// Error type carrying an exit code + JSON-awareness (rule-cli §7, §13).
// ---------------------------------------------------------------------------

pub(crate) struct AppError {
    pub(crate) message: String,
    pub(crate) code: i32,
    /// `true` → render `syntax error:` rather than `error:`.
    pub(crate) syntax: bool,
}

impl AppError {
    pub(crate) fn usage(msg: impl Into<String>) -> AppError {
        AppError { message: msg.into(), code: 1, syntax: false }
    }
    pub(crate) fn syntax(msg: impl Into<String>) -> AppError {
        AppError { message: msg.into(), code: 2, syntax: true }
    }
}

impl From<RuleError> for AppError {
    fn from(e: RuleError) -> Self {
        AppError { message: e.to_string(), code: e.exit_code(), syntax: e.is_syntax() }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        // Map a typed RuleError carried through anyhow back to its code.
        if let Some(re) = e.downcast_ref::<RuleError>() {
            return AppError {
                message: re.to_string(),
                code: re.exit_code(),
                syntax: re.is_syntax(),
            };
        }
        AppError::usage(format!("{e:#}"))
    }
}

fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if json {
                eprintln!("{}", json!({ "error": e.message, "code": e.code }));
            } else if e.syntax {
                eprintln!("syntax error: {}", e.message);
            } else {
                eprintln!("error: {}", e.message);
            }
            ExitCode::from(e.code as u8)
        }
    }
}

fn run(args: &[String]) -> Result<(), AppError> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    let verb = args[0].as_str();
    let flags = Flags::parse(&args[1..])?;

    match verb {
        "list" => commands::cmd_list(&flags),
        "show" => commands::cmd_show(&flags),
        "test" => commands::cmd_test(&flags),
        "add" => commands::cmd_add(&flags),
        "edit" => commands::cmd_edit(&flags),
        "rm" => commands::cmd_rm(&flags),
        "move" => commands::cmd_move(&flags),
        "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(AppError::usage(format!("unknown verb {other:?}. Try `rule --help`."))),
    }
}

fn print_help() {
    println!(
        "rule — view and edit normalisation rules (scriptable; library-backed)\n\
\n\
USAGE\n  \
  rule list   --stage <stage> [--json]\n  \
  rule show   --stage <stage> --id <id> [--json]\n  \
  rule test   --stage <stage> [candidate flags] \"<string>\"\n  \
  rule add    --stage <stage> [field flags] [--apply|-a] [--json]\n  \
  rule edit   --stage <stage> --id <id> [field flags] [--apply|-a] [--json]\n  \
  rule rm     --stage <stage> --id <id> [--apply --force | -af] [--json]\n  \
  rule move   --stage <stage> --id <id> (--before <id> | --after <id>) [--apply|-a]\n\
\n\
STAGES\n  \
  prefixes suffixes expansions  (loop — ordered, reorder with `move`)\n  \
  persons employers merchants banking_ops  (first-match-wins, auto-ordered)\n  \
  locations  (additive)\n\
\n\
DEFAULTS\n  \
  add/edit/rm/move are DRY-RUN (evaluate) unless --apply/-a is given.\n  \
  rm additionally requires --force/-f to commit (combine as -af).\n\
\n\
FIELD FLAGS (validated per stage; --stage is always required)\n  \
  values:   --pattern --canonical --operation --gateway --institution --kind --note\n  \
  features: --has-account --has-date --has-location --has-currency-code --has-amount\n            \
  (negatives --no-* only meaningful on `edit`; a --has-* feature\n             \
  requires the matching (?P<name>...) capture group in --pattern)\n\
\n\
Colour/bold on a TTY; plain when piped, --json, or NO_COLOR set."
    );
}
