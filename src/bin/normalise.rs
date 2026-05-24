use std::env;

use anyhow::{bail, Result};

use pocketsmith_sync::db;
use pocketsmith_sync::db::payee_normalisations as pn;
use pocketsmith_sync::normalise::{apply, scan};
use pocketsmith_sync::transfers::Status;

/// `normalise` — scan/apply review flow for payee normalisations.
///
/// Default mode (no args): SCAN.
///   Walk every unique `original_payee` in `transactions`, run the
///   normalisation pipeline, and stage proposals into the
///   `payee_normalisations` table per the policy in PLAN.md. Idempotent —
///   re-running just refreshes proposals and txn counts.
///
/// `--apply`: APPLY.
///   Drain rows whose status is `confirmed`: write `transactions.payee` and
///   delete the staging row. Rejected rows persist (they suppress
///   re-prompting on the next scan).
///
/// Review confirm/reject decisions happen out-of-band via `serve`.
fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let mut apply_mode = false;
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--apply" => apply_mode = true,
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => bail!("unknown argument: {other}. Try --help."),
        }
    }

    let conn = db::initialize("pocketsmith.db")?;

    if apply_mode {
        run_apply(&conn)
    } else {
        run_scan(&conn)
    }
}

fn print_help() {
    println!(
        "usage:\n  \
         normalise            scan transactions, stage proposals in payee_normalisations\n  \
         normalise --apply    drain confirmed proposals (write transactions.payee, delete row)\n\n\
         Review and confirm/reject pending proposals through `serve`."
    );
}

fn run_scan(conn: &rusqlite::Connection) -> Result<()> {
    let stats = scan::scan(conn)?;
    println!("=== Normalise scan ===");
    println!("  inserted (new pending proposal):     {}", stats.inserted);
    println!("  overwritten (proposal changed):      {}", stats.overwritten);
    println!("  txn_count updated (unchanged prop.): {}", stats.txn_count_updated);
    println!("  skipped (already in sync):           {}", stats.skipped_no_change);

    let counts = pn::count_by_status(conn)?;
    let pending = counts.get(&Status::Pending).copied().unwrap_or(0);
    let confirmed = counts.get(&Status::Confirmed).copied().unwrap_or(0);
    let rejected = counts.get(&Status::Rejected).copied().unwrap_or(0);
    println!("\n=== Staging table totals ===");
    println!("  pending:   {pending}");
    println!("  confirmed: {confirmed}  (run `normalise --apply` to drain)");
    println!("  rejected:  {rejected}");
    Ok(())
}

fn run_apply(conn: &rusqlite::Connection) -> Result<()> {
    let stats = apply::apply_confirmed(conn)?;
    println!("=== Normalise apply ===");
    println!("  transactions updated: {}", stats.transactions_updated);
    println!("  staging rows drained: {}", stats.rows_drained);
    Ok(())
}
