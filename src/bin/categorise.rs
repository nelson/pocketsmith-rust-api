use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

use pocketsmith_sync::categorise::audit;
use pocketsmith_sync::categorise::eval;
use pocketsmith_sync::categorise::pipeline::{self, PipelineConfig};
use pocketsmith_sync::categorise::rules::CategoriseRules;
use pocketsmith_sync::normalise::main::PipelineRules;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let auto_approve = std::env::args().any(|a| a == "--yes");

    let conn = pocketsmith_sync::db::initialize("pocketsmith.db")?;

    // Migrate any existing places_cache rows into categorisation_audit
    let migrated = audit::migrate_places_cache(&conn)?;
    if migrated > 0 {
        println!("Migrated {} places_cache entries to categorisation_audit.", migrated);
    }
    let normalise_rules = PipelineRules::load(Path::new("rules"))?;
    let categorise_rules = CategoriseRules::load(Path::new("rules"))?;

    let config = PipelineConfig {
        google_places_key: std::env::var("GOOGLE_PLACES_API_KEY").ok(),
        anthropic_key: std::env::var("ANTHROPIC_API_KEY").ok(),
    };

    let (results, changes) = pipeline::run(&conn, &normalise_rules, &categorise_rules, &config)?;

    let report = eval::build_report(&results, &changes);
    eval::print_report(&report);
    eval::print_changes(&changes, 30);

    if changes.is_empty() {
        return Ok(());
    }

    println!(
        "\nApply {} changes to {} transactions?",
        report.total_changes, report.total_txns_affected
    );

    if !auto_approve {
        print!("[y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    } else {
        println!("Auto-approving (--yes)");
    }

    let applied = pipeline::apply_changes(&conn, &changes)?;
    println!("Applied {} transaction updates.", applied);

    Ok(())
}
