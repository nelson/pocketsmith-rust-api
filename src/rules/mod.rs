//! Filesystem-canonical store for the editable normalisation rules
//! (editable-rules-v3 §6).
//!
//! The canonical copy of each pipeline stage's rule table lives in
//! SQLite, and is mirrored to `src/rules/<stage>.sql` so that:
//!   * git diffs of rule edits are human-reviewable, and
//!   * a blown-away database can be re-seeded with the *edited* rules
//!     (not just the original in-code constants).
//!
//! Lifecycle (§6.1):
//!   * On serve startup, [`load_into_db`] seeds any empty rule table
//!     from its `src/rules/*.sql` file (falling back to the in-code
//!     constants the very first time, before any file exists).
//!   * On every committed rule mutation, [`schedule_dump`] re-dumps the
//!     affected stage on a background thread.
//!
//! In PR 1 this module is pure infrastructure: nothing reads the rule
//! tables for normalisation yet (the pipeline still uses its in-code
//! `const` dictionaries). Stage conversions land in later PRs.

pub mod seed;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Bump when an in-tree `src/rules/*.sql` file gains a new column, so a
/// startup load knows to re-seed. Stored in `_meta.rules_schema_version`.
pub const RULES_SCHEMA_VERSION: i64 = 1;

/// The eight editable pipeline stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Prefixes,
    Suffixes,
    Expansions,
    Persons,
    Employers,
    Merchants,
    BankingOps,
    Locations,
}

impl Stage {
    /// All stages, in pipeline execution order.
    pub fn all() -> [Stage; 8] {
        [
            Stage::Prefixes,
            Stage::Suffixes,
            Stage::Expansions,
            Stage::Persons,
            Stage::Employers,
            Stage::Merchants,
            Stage::BankingOps,
            Stage::Locations,
        ]
    }

    /// SQLite table name backing this stage.
    pub fn table(&self) -> &'static str {
        match self {
            Stage::Prefixes => "rule_prefixes",
            Stage::Suffixes => "rule_suffixes",
            Stage::Expansions => "rule_expansions",
            Stage::Persons => "rule_persons",
            Stage::Employers => "rule_employers",
            Stage::Merchants => "rule_merchants",
            Stage::BankingOps => "rule_banking_ops",
            Stage::Locations => "rule_locations",
        }
    }

    /// File stem for `src/rules/<stem>.sql`.
    pub fn file_stem(&self) -> &'static str {
        match self {
            Stage::Prefixes => "prefixes",
            Stage::Suffixes => "suffixes",
            Stage::Expansions => "expansions",
            Stage::Persons => "persons",
            Stage::Employers => "employers",
            Stage::Merchants => "merchants",
            Stage::BankingOps => "banking_ops",
            Stage::Locations => "locations",
        }
    }

    /// Content columns dumped to / loaded from the canonical SQL file.
    /// Deliberately excludes `id`, `created_at`, `updated_at`: ids are
    /// re-assigned by insertion order on reload (so declaration order
    /// is preserved), and timestamps fall back to their column DEFAULT.
    fn dump_columns(&self) -> &'static [&'static str] {
        match self {
            Stage::Prefixes => &[
                "pattern", "gateway", "operation", "has_account", "has_date", "note",
                "sort_order",
            ],
            Stage::Suffixes => &[
                "pattern", "gateway", "operation", "institution", "has_account", "has_date",
                "has_location", "has_currency_code", "has_amount", "note", "sort_order",
            ],
            Stage::Expansions => &["pattern", "canonical", "note", "sort_order"],
            Stage::Persons => &["canonical", "pattern", "note"],
            Stage::Employers => &["canonical", "pattern", "note"],
            Stage::Merchants => &["canonical", "pattern", "note"],
            Stage::BankingOps => &["operation", "pattern", "has_account", "note", "sort_order"],
            Stage::Locations => &["location", "note"],
        }
    }

    /// Stable ORDER BY for dumps. Loop stages order by `sort_order` then
    /// id; the rest by id (= insertion = declaration order), so a
    /// dump → load → dump round-trip is byte-identical.
    fn dump_order_by(&self) -> &'static str {
        match self {
            Stage::Prefixes | Stage::Suffixes | Stage::Expansions | Stage::BankingOps => {
                "sort_order, id"
            }
            _ => "id",
        }
    }
}

/// Directory holding the canonical `*.sql` files. Defaults to
/// `src/rules` (relative to the process cwd, i.e. the repo root for
/// `cargo run`). Overridable via `POCKETSMITH_RULES_DIR` (used by
/// tests and isolated runs).
pub fn rules_dir() -> PathBuf {
    std::env::var("POCKETSMITH_RULES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("src/rules"))
}

fn row_count(conn: &Connection, stage: Stage) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {}", stage.table());
    Ok(conn.query_row(&sql, [], |r| r.get(0))?)
}

/// Seed any empty rule table on startup (§6.1). For each stage whose
/// table is empty we load its `src/rules/<stage>.sql` file; if the file
/// is missing (very first boot, before any dump) we fall back to the
/// in-code constants via [`bootstrap_stage`].
///
/// Tables that already hold rows are left untouched, so an existing DB
/// retains its (possibly UI-edited) data.
pub fn load_into_db(conn: &Connection) -> Result<()> {
    let dir = rules_dir();
    for stage in Stage::all() {
        if row_count(conn, stage)? > 0 {
            continue;
        }
        let file = dir.join(format!("{}.sql", stage.file_stem()));
        if file.exists() {
            let sql = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            conn.execute_batch(&sql)
                .with_context(|| format!("loading {}", file.display()))?;
        } else {
            bootstrap_stage(conn, stage)?;
        }
    }
    set_schema_version(conn, RULES_SCHEMA_VERSION)?;
    Ok(())
}

fn set_schema_version(conn: &Connection, v: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('rules_schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![v.to_string()],
    )?;
    Ok(())
}

/// SQLite string literal: wrap in single quotes, double embedded quotes.
fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Render a fetched column value as a SQL literal for the dump file.
fn render_value(v: &rusqlite::types::Value) -> String {
    use rusqlite::types::Value;
    match v {
        Value::Null => "NULL".to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => sql_str(s),
        Value::Blob(_) => panic!("rule tables never store blobs"),
    }
}

/// Serialise one stage's table to a SQL string (the contents of
/// `src/rules/<stage>.sql`). Pure — does no file I/O — so it's easy to
/// unit-test and so the background dumper can read the DB independently.
pub fn dump_stage_to_string(conn: &Connection, stage: Stage) -> Result<String> {
    let cols = stage.dump_columns();
    let collist = cols.join(", ");
    let sql = format!(
        "SELECT {} FROM {} ORDER BY {}",
        collist,
        stage.table(),
        stage.dump_order_by()
    );
    let mut stmt = conn.prepare(&sql)?;
    let n = cols.len();
    let rows = stmt.query_map([], |row| {
        let mut vals = Vec::with_capacity(n);
        for i in 0..n {
            vals.push(row.get::<_, rusqlite::types::Value>(i)?);
        }
        Ok(vals)
    })?;

    let mut out = String::new();
    out.push_str(&format!(
        "-- Canonical seed for the `{}` pipeline stage.\n\
         -- Generated by `cargo run --bin dump_rules` (editable-rules-v3 §6).\n\
         -- Edit via the Pipeline tab; this file is re-dumped on each change.\n",
        stage.file_stem()
    ));
    for r in rows {
        let vals = r?;
        let rendered: Vec<String> = vals.iter().map(render_value).collect();
        out.push_str(&format!(
            "INSERT INTO {} ({}) VALUES ({});\n",
            stage.table(),
            collist,
            rendered.join(", ")
        ));
    }
    Ok(out)
}

/// Write one stage's canonical SQL file to `src/rules/<stage>.sql`.
pub fn dump_stage(conn: &Connection, stage: Stage) -> Result<()> {
    let dir = rules_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let body = dump_stage_to_string(conn, stage)?;
    let path = dir.join(format!("{}.sql", stage.file_stem()));
    write_atomic(&path, body.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Dump every stage. Used by the `dump_rules` bootstrap binary.
pub fn dump_all(conn: &Connection) -> Result<()> {
    for stage in Stage::all() {
        dump_stage(conn, stage)?;
    }
    Ok(())
}

/// Serialise concurrent background dumps. tiny_http is single-threaded
/// today, but the detached dump threads could otherwise race on the
/// same file; the mutex makes last-write-wins safe (§6.1).
static DUMP_LOCK: Mutex<()> = Mutex::new(());

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let _guard = DUMP_LOCK.lock().unwrap();
    let tmp = path.with_extension("sql.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Re-dump one stage on a detached background thread (§6.1). Opens its
/// own connection so it can read committed DB state without blocking
/// the HTTP response. Errors are logged, not propagated — the file is a
/// backup, not the source of truth while serve is running.
pub fn schedule_dump(stage: Stage, db_path: String) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let conn = Connection::open(&db_path)?;
            dump_stage(&conn, stage)
        })();
        if let Err(e) = result {
            eprintln!("rules: background dump of {:?} failed: {e:#}", stage);
        }
    });
}

// ---------------------------------------------------------------------
// Bootstrap from the in-code constants (first-ever boot / fidelity oracle)
// ---------------------------------------------------------------------

/// Seed every rule table from the in-code constant dictionaries. Used
/// to generate the first `src/rules/*.sql` files and as the load
/// fallback before any file exists.
pub fn bootstrap_from_constants(conn: &Connection) -> Result<()> {
    for stage in Stage::all() {
        bootstrap_stage(conn, stage)?;
    }
    Ok(())
}

/// Seed a single stage from its in-code constants. Idempotent only on an
/// empty table (callers guard on emptiness).
pub fn bootstrap_stage(conn: &Connection, stage: Stage) -> Result<()> {
    use crate::normalise;
    match stage {
        Stage::Prefixes => {
            for (i, r) in normalise::prefix::seed_rows().into_iter().enumerate() {
                conn.execute(
                    "INSERT INTO rule_prefixes (pattern, gateway, operation, has_account, has_date, sort_order)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![r.pattern, r.gateway, r.operation, r.has_account as i64, r.has_date as i64, i as i64],
                )?;
            }
        }
        Stage::Suffixes => {
            for (i, r) in normalise::suffix::seed_rows().into_iter().enumerate() {
                conn.execute(
                    "INSERT INTO rule_suffixes (pattern, gateway, operation, institution, has_account, has_date, has_location, has_currency_code, has_amount, sort_order)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        r.pattern, r.gateway, r.operation, r.institution,
                        r.has_account as i64, r.has_date as i64, r.has_location as i64,
                        r.has_currency_code as i64, r.has_amount as i64, i as i64
                    ],
                )?;
            }
        }
        Stage::Expansions => {
            for (i, r) in normalise::expand::seed_rows().into_iter().enumerate() {
                conn.execute(
                    "INSERT INTO rule_expansions (pattern, canonical, sort_order) VALUES (?1, ?2, ?3)",
                    rusqlite::params![r.pattern, r.canonical, i as i64],
                )?;
            }
        }
        Stage::Persons => {
            for r in normalise::persons::seed_rows() {
                conn.execute(
                    "INSERT INTO rule_persons (canonical, pattern) VALUES (?1, ?2)",
                    rusqlite::params![r.canonical, r.pattern],
                )?;
            }
        }
        Stage::Employers => {
            for r in normalise::employers::seed_rows() {
                conn.execute(
                    "INSERT INTO rule_employers (canonical, pattern) VALUES (?1, ?2)",
                    rusqlite::params![r.canonical, r.pattern],
                )?;
            }
        }
        Stage::Merchants => {
            for r in normalise::merchants::seed_rows() {
                conn.execute(
                    "INSERT INTO rule_merchants (canonical, pattern) VALUES (?1, ?2)",
                    rusqlite::params![r.canonical, r.pattern],
                )?;
            }
        }
        Stage::BankingOps => {
            for (i, r) in normalise::banking_ops::seed_rows().into_iter().enumerate() {
                conn.execute(
                    "INSERT INTO rule_banking_ops (operation, pattern, has_account, sort_order) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![r.operation, r.pattern, r.has_account as i64, i as i64],
                )?;
            }
        }
        Stage::Locations => {
            for loc in normalise::locations::seed_rows() {
                conn.execute(
                    "INSERT INTO rule_locations (location) VALUES (?1)",
                    rusqlite::params![loc],
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;

    #[test]
    fn bootstrap_populates_all_stages() {
        let conn = initialize_in_memory().unwrap();
        bootstrap_from_constants(&conn).unwrap();
        for stage in Stage::all() {
            assert!(
                row_count(&conn, stage).unwrap() > 0,
                "stage {:?} should have rows after bootstrap",
                stage
            );
        }
    }

    #[test]
    fn prefix_sort_order_matches_declaration_order() {
        let conn = initialize_in_memory().unwrap();
        bootstrap_stage(&conn, Stage::Prefixes).unwrap();
        // First declared prefix is the date prefix.
        let first: String = conn
            .query_row(
                "SELECT pattern FROM rule_prefixes ORDER BY sort_order LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(first.contains("date"), "unexpected first prefix: {first}");
        // sort_order is dense 0..N-1.
        let (min, max, cnt): (i64, i64, i64) = conn
            .query_row(
                "SELECT MIN(sort_order), MAX(sort_order), COUNT(*) FROM rule_prefixes",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(min, 0);
        assert_eq!(max, cnt - 1);
    }

    #[test]
    fn dump_round_trips_identically() {
        // dump → load into a fresh DB → dump again ⇒ byte-identical.
        let conn = initialize_in_memory().unwrap();
        bootstrap_from_constants(&conn).unwrap();
        for stage in Stage::all() {
            let first = dump_stage_to_string(&conn, stage).unwrap();

            let conn2 = initialize_in_memory().unwrap();
            conn2.execute_batch(&first).unwrap();
            let second = dump_stage_to_string(&conn2, stage).unwrap();

            assert_eq!(first, second, "round-trip differs for {:?}", stage);
        }
    }

    #[test]
    fn load_into_db_is_noop_when_tables_populated() {
        let conn = initialize_in_memory().unwrap();
        bootstrap_from_constants(&conn).unwrap();
        let before: i64 = row_count(&conn, Stage::Merchants).unwrap();
        // POCKETSMITH_RULES_DIR points nowhere meaningful here, but since
        // tables are populated load_into_db must not touch them.
        load_into_db(&conn).unwrap();
        let after: i64 = row_count(&conn, Stage::Merchants).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn committed_seed_files_match_constants() {
        // The committed src/rules/*.sql files must load cleanly and
        // reproduce exactly what the in-code constants bootstrap.
        let manifest = env!("CARGO_MANIFEST_DIR");
        for stage in Stage::all() {
            let path = std::path::Path::new(manifest)
                .join("src/rules")
                .join(format!("{}.sql", stage.file_stem()));
            let sql = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

            let from_file = initialize_in_memory().unwrap();
            from_file.execute_batch(&sql).unwrap();

            let from_const = initialize_in_memory().unwrap();
            bootstrap_stage(&from_const, stage).unwrap();

            assert_eq!(
                dump_stage_to_string(&from_file, stage).unwrap(),
                dump_stage_to_string(&from_const, stage).unwrap(),
                "committed seed for {:?} drifted from constants \u{2014} re-run `cargo run --bin dump_rules`",
                stage
            );
        }
    }

    #[test]
    fn load_into_db_falls_back_to_constants_when_no_files() {
        // Point at an empty temp dir so no *.sql files exist; load must
        // fall back to the in-code constants.
        let tmp = std::env::temp_dir().join(format!("ps-rules-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("POCKETSMITH_RULES_DIR", &tmp);

        let conn = initialize_in_memory().unwrap();
        load_into_db(&conn).unwrap();
        assert!(row_count(&conn, Stage::Merchants).unwrap() > 0);

        std::env::remove_var("POCKETSMITH_RULES_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
