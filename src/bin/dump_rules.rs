//! `dump_rules` — regenerate the canonical `src/rules/*.sql` files from
//! the in-code constant dictionaries (editable-rules-v3 §6.2).
//!
//! Builds an in-memory database, seeds every rule table from the in-code
//! constants, and dumps all eight stages to `src/rules/<stage>.sql`.
//! This is the bootstrap that produces the first version of those files
//! (and an escape hatch to regenerate them from the constants).
//!
//! Once serve is editing rules, the live `src/rules/*.sql` files are
//! kept current by serve's per-mutation background dumps; running this
//! binary would overwrite UI edits with the in-code constants, so it is
//! intended for bootstrap / recovery only.
//!
//! Output directory is `src/rules` by default; override with
//! `POCKETSMITH_RULES_DIR`.

use anyhow::Result;
use pocketsmith_sync::db::initialize_in_memory;
use pocketsmith_sync::rules;

fn main() -> Result<()> {
    let conn = initialize_in_memory()?;
    rules::bootstrap_from_constants(&conn)?;
    rules::dump_all(&conn)?;

    let dir = rules::rules_dir();
    println!("Wrote 8 rule files to {}/", dir.display());
    for stage in rules::Stage::all() {
        let path = dir.join(format!("{}.sql", stage.file_stem()));
        let n = std::fs::read_to_string(&path)
            .map(|s| s.lines().filter(|l| l.starts_with("INSERT")).count())
            .unwrap_or(0);
        println!("  {:<16} {:>4} rules", stage.file_stem(), n);
    }
    Ok(())
}
