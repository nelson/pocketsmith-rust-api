//! `dump` — export the live database's rule tables to the canonical
//! `rules/*.sql` files (editable-rules-v3 §6.2).
//!
//! Reads the rule tables from the database at `POCKETSMITH_DB` (the same
//! DB serve uses) and dumps all eight stages to `rules/<stage>.sql`.
//! Use it to snapshot the current (possibly UI-edited) rules to disk, or
//! to recover the canonical files after a manual DB edit.
//!
//! Serve already re-dumps each stage in the background on every rule
//! mutation, so you rarely need to run this by hand — it's the explicit
//! export / recovery path.
//!
//! Output directory is `rules` by default; override with
//! `POCKETSMITH_RULES_DIR`.

use anyhow::Result;
use pocketsmith_sync::{db, rules};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let db_path = db::path_from_env();
    let conn = db::open_app_db_at(&db_path)?;
    rules::dump_all(&conn)?;

    let dir = rules::rules_dir();
    println!("Dumped 8 rule files from {db_path} to {}/", dir.display());
    for stage in rules::Stage::all() {
        let path = dir.join(format!("{}.sql", stage.name()));
        let n = std::fs::read_to_string(&path)
            .map(|s| s.lines().filter(|l| l.starts_with("INSERT")).count())
            .unwrap_or(0);
        println!("  {:<16} {:>4} rules", stage.name(), n);
    }
    Ok(())
}
