//! `pocketsmith push` — push Stage-1 confirmed transfer changes to
//! Pocketsmith. See `src/push/mod.rs` for behaviour and `.claude/plans/
//! push-stage-1-transfer-pair-mvp.md` for design.

use std::env;
use std::process::ExitCode;

use anyhow::{Context, Result};

use pocketsmith::client::PocketSmithClient;
use pocketsmith::db;
use pocketsmith::push::{self, PushOpts};

pub fn run(args: &[String]) -> ExitCode {
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let opts = match push::parse_args(&argv) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("usage: pocketsmith push [--dry-run] [--limit N]");
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    match run_inner(&opts) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("push: fatal: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run_inner(opts: &PushOpts) -> Result<ExitCode> {
    let api_key = env::var("POCKETSMITH_API_KEY")
        .context("POCKETSMITH_API_KEY not set (see .env.example)")?;
    let client = PocketSmithClient::new(api_key);
    let conn = db::open_app_db()?;

    let stats = push::push(&client, &conn, opts)?;

    println!("=== push summary ===");
    if opts.dry_run {
        println!("(dry-run — no PUTs issued)");
    }
    println!("pushed:                   {}", stats.pushed);
    println!("would_push:               {}", stats.would_push);
    println!("skipped_changed_upstream: {}", stats.skipped_changed_upstream);
    println!("deleted_upstream:         {}", stats.deleted_upstream);
    println!("failed:                   {}", stats.failed);

    // Non-zero exit only on hard failures. `skipped_changed_upstream` and
    // `deleted_upstream` are expected outcomes, not errors.
    Ok(if stats.failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
