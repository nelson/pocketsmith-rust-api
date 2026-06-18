use std::env;

use anyhow::{bail, Result};
use rusqlite::Connection;

use pocketsmith_sync::categorise::apply;
use pocketsmith_sync::categorise::places::{self, GooglePlacesClient};
use pocketsmith_sync::categorise::scan;
use pocketsmith_sync::db;
use pocketsmith_sync::db::category_proposals as cp;
use pocketsmith_sync::review::Status;

/// `categorise` — the final pipeline stage: assign Pocketsmith categories
/// + labels to merchant transactions, sourced from Google Places.
///
/// Subcommands:
///   scan                cache-first Places lookups, stage pending proposals
///   list [--status S]   show staged proposals (text, or --json)
///   confirm <key>       mark a proposal confirmed (use --all for every pending)
///   reject  <key>       mark a proposal rejected
///   apply               write confirmed category+labels to transactions
///   lookup "<query>"    ad-hoc Places probe (uses + fills the cache)
///
/// `scan` and `lookup` require GOOGLE_PLACES_API_KEY; `list`/`confirm`/
/// `reject`/`apply` never touch the network.
fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("scan");
    let rest = &args[args.len().min(1)..];

    let conn = db::open_app_db()?;

    match cmd {
        "scan" => run_scan(&conn),
        "list" => run_list(&conn, rest),
        "confirm" => run_set_status(&conn, rest, Status::Confirmed),
        "reject" => run_set_status(&conn, rest, Status::Rejected),
        "apply" => run_apply(&conn),
        "lookup" => run_lookup(&conn, rest),
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        other => {
            bail!("unknown subcommand: {other}. Try `categorise --help`.");
        }
    }
}

fn print_help() {
    println!(
        "usage:\n  \
         categorise scan                 scan merchant txns, stage proposals\n  \
         categorise list [--status S]    list proposals (S = pending|confirmed|rejected); --json\n  \
         categorise confirm <key|--all>  confirm a proposal (or all pending)\n  \
         categorise reject  <key>        reject a proposal\n  \
         categorise apply                write confirmed category+labels to transactions\n  \
         categorise lookup \"<query>\"      ad-hoc Places probe (fills the cache); --json\n\n\
         scan + lookup need GOOGLE_PLACES_API_KEY (see .env)."
    );
}

fn run_scan(conn: &Connection) -> Result<()> {
    let client = GooglePlacesClient::from_env()?;
    let stats = scan::scan(conn, &client)?;
    println!("=== Categorise scan ===");
    println!("  merchants seen:                  {}", stats.merchants_seen);
    println!("  inserted (new pending proposal): {}", stats.inserted);
    println!("  overwritten (proposal changed):  {}", stats.overwritten);
    println!("  txn_count updated (unchanged):   {}", stats.txn_count_updated);
    println!("  skipped (unmapped place type):   {}", stats.skipped_unmapped);
    println!("  skipped (lookup error, will retry): {}", stats.skipped_error);

    print_totals(conn)?;
    Ok(())
}

fn print_totals(conn: &Connection) -> Result<()> {
    let counts = cp::count_by_status(conn)?;
    let pending = counts.get(&Status::Pending).copied().unwrap_or(0);
    let confirmed = counts.get(&Status::Confirmed).copied().unwrap_or(0);
    let rejected = counts.get(&Status::Rejected).copied().unwrap_or(0);
    println!("\n=== Staging table totals ===");
    println!("  pending:   {pending}");
    println!("  confirmed: {confirmed}  (run `categorise apply` to drain)");
    println!("  rejected:  {rejected}");
    Ok(())
}

fn run_list(conn: &Connection, args: &[String]) -> Result<()> {
    let mut status: Option<Status> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--status" => {
                i += 1;
                let s = args.get(i).map(|s| s.as_str()).unwrap_or("");
                status = Some(Status::from_str(s).ok_or_else(|| {
                    anyhow::anyhow!("invalid status '{s}' (pending|confirmed|rejected)")
                })?);
            }
            other => bail!("unknown flag for list: {other}"),
        }
        i += 1;
    }

    let rows = match status {
        Some(s) => cp::list_by_status(conn, s)?,
        None => cp::list_all(conn)?,
    };

    if json {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "merchant_key": r.merchant_key,
                    "category_id": r.proposed_category,
                    "category_title": category_title(conn, r.proposed_category).ok().flatten(),
                    "labels": r.proposed_labels,
                    "place_type": r.place_type,
                    "txn_count": r.txn_count,
                    "status": r.status.as_str(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("(no proposals)");
        return Ok(());
    }
    for r in &rows {
        let cat = category_title(conn, r.proposed_category)?
            .unwrap_or_else(|| "(unmapped)".to_string());
        let labels = if r.proposed_labels.is_empty() {
            "-".to_string()
        } else {
            r.proposed_labels.join(",")
        };
        println!(
            "[{}] {:<6} {:<4} {:<14} {:<10} {}",
            status_glyph(r.status),
            r.txn_count,
            r.place_type.as_deref().unwrap_or("-"),
            cat,
            labels,
            r.merchant_key,
        );
    }
    Ok(())
}

fn run_set_status(conn: &Connection, args: &[String], status: Status) -> Result<()> {
    if args.first().map(|s| s.as_str()) == Some("--all") {
        if status != Status::Confirmed {
            bail!("--all is only supported for confirm");
        }
        let pending = cp::list_by_status(conn, Status::Pending)?;
        for r in &pending {
            cp::update_status(conn, &r.merchant_key, Status::Confirmed)?;
        }
        println!("confirmed {} pending proposal(s)", pending.len());
        return Ok(());
    }

    let key = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("expected a merchant key (see `categorise list`)"))?;
    if cp::get(conn, key)?.is_none() {
        bail!("no proposal with merchant key '{key}'");
    }
    cp::update_status(conn, key, status)?;
    println!("{} {key}", status.as_str());
    Ok(())
}

fn run_apply(conn: &Connection) -> Result<()> {
    let stats = apply::apply_confirmed(conn)?;
    println!("=== Categorise apply ===");
    println!("  transactions updated: {}", stats.transactions_updated);
    println!("  staging rows drained: {}", stats.rows_drained);
    Ok(())
}

fn run_lookup(conn: &Connection, args: &[String]) -> Result<()> {
    let json = args.iter().any(|a| a == "--json");
    let query = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("expected a query string"))?;
    let client = GooglePlacesClient::from_env()?;
    let row = places::lookup(conn, &client, query)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "query": row.query,
                "place_id": row.place_id,
                "display_name": row.display_name,
                "primary_type": row.primary_type,
                "types": row.types,
                "status": row.status.as_str(),
            }))?
        );
    } else {
        println!("query:        {}", row.query);
        println!("status:       {}", row.status.as_str());
        println!("display_name: {}", row.display_name.as_deref().unwrap_or("-"));
        println!("primary_type: {}", row.primary_type.as_deref().unwrap_or("-"));
        println!("types:        {}", row.types.join(", "));
    }
    Ok(())
}

fn category_title(conn: &Connection, id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    use rusqlite::OptionalExtension;
    let title = conn
        .query_row(
            "SELECT title FROM categories WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(title)
}

fn status_glyph(s: Status) -> char {
    match s {
        Status::Pending => '?',
        Status::Confirmed => 'Y',
        Status::Rejected => 'N',
    }
}
