//! Filesystem-canonical store for the editable normalisation rules
//! (editable-rules-v3 §6).
//!
//! The canonical copy of each pipeline stage's rule table lives in
//! SQLite, and is mirrored to `src/rules/<stage>.sql` so that:
//!   * git diffs of rule edits are human-reviewable, and
//!   * a blown-away database can be re-seeded with the *edited* rules.
//!
//! The `src/rules/*.sql` files — not the in-code `const` dictionaries —
//! are the source of truth for the seed. (The constants still drive the
//! pipeline until each stage is converted in later PRs, but they no
//! longer feed the rule tables.)
//!
//! Lifecycle (§6.1):
//!   * On serve startup, [`load_into_db`] seeds any empty rule table
//!     from its `src/rules/*.sql` file.
//!   * On every committed rule mutation, [`schedule_dump`] re-dumps the
//!     affected stage on a background thread.
//!   * `cargo run --bin dump` dumps the *live* DB to the `*.sql` files
//!     (bootstrap / recovery / manual export).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
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

    /// Short name: the `src/rules/<name>.sql` file stem and the suffix of
    /// the `rule_<name>` table. The single source for both (see
    /// [`table`](Self::table)).
    pub fn name(&self) -> &'static str {
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

    /// SQLite table name backing this stage: always `rule_<name>`.
    pub fn table(&self) -> String {
        format!("rule_{}", self.name())
    }

    /// Parse a stage from its `name` (used in `/pipeline/stage/<x>`
    /// URLs). Returns `None` for an unknown slug.
    pub fn from_name(s: &str) -> Option<Stage> {
        Stage::all().into_iter().find(|st| st.name() == s)
    }

    /// Content columns dumped to / loaded from the canonical SQL file.
    /// Excludes `id` (re-assigned by insertion order on reload, so
    /// declaration order is preserved). `created_at` / `updated_at` are
    /// included so a rule's age survives a re-seed — seed rows carry a
    /// fixed historical timestamp; UI edits stamp the edit time.
    fn dump_columns(&self) -> &'static [&'static str] {
        match self {
            Stage::Prefixes => &[
                "pattern", "gateway", "operation", "has_account", "has_date", "note",
                "sort_order", "created_at", "updated_at",
            ],
            Stage::Suffixes => &[
                "pattern", "gateway", "operation", "institution", "has_account", "has_date",
                "has_location", "has_currency_code", "has_amount", "note", "sort_order",
                "created_at", "updated_at",
            ],
            Stage::Expansions => {
                &["pattern", "canonical", "note", "sort_order", "created_at", "updated_at"]
            }
            Stage::Persons => &["canonical", "pattern", "note", "created_at", "updated_at"],
            Stage::Employers => &["canonical", "pattern", "note", "created_at", "updated_at"],
            Stage::Merchants => &["canonical", "pattern", "note", "created_at", "updated_at"],
            Stage::BankingOps => &[
                "operation", "pattern", "has_account", "note", "sort_order", "created_at",
                "updated_at",
            ],
            Stage::Locations => &["location", "kind", "note", "created_at", "updated_at"],
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

/// Number of rules currently stored for `stage`.
pub fn count(conn: &Connection, stage: Stage) -> Result<i64> {
    row_count(conn, stage)
}

/// Seed any empty rule table on startup (§6.1) from its
/// `src/rules/<stage>.sql` file. Tables that already hold rows are left
/// untouched, so an existing DB retains its (possibly UI-edited) data.
pub fn load_into_db(conn: &Connection) -> Result<()> {
    let dir = rules_dir();
    for stage in Stage::all() {
        if row_count(conn, stage)? > 0 {
            continue;
        }
        let file = dir.join(format!("{}.sql", stage.name()));
        if !file.exists() {
            bail!(
                "missing canonical seed file {} \u{2014} run `cargo run --bin dump` \
                 from a populated DB to regenerate it",
                file.display()
            );
        }
        let sql = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        conn.execute_batch(&sql)
            .with_context(|| format!("loading {}", file.display()))?;
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
    let table = stage.table();
    let sql = format!(
        "SELECT {} FROM {} ORDER BY {}",
        collist,
        table,
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
         -- Generated by `cargo run --bin dump` (editable-rules-v3 §6).\n\
         -- Edit via the Pipeline tab; this file is re-dumped on each change.\n",
        stage.name()
    ));
    for r in rows {
        let vals = r?;
        let rendered: Vec<String> = vals.iter().map(render_value).collect();
        out.push_str(&format!(
            "INSERT INTO {} ({}) VALUES ({});\n",
            table,
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
    let path = dir.join(format!("{}.sql", stage.name()));
    write_atomic(&path, body.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Dump every stage. Used by the `dump` binary.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;

    /// Load the committed `src/rules/<stage>.sql` into a fresh DB.
    fn load_committed(stage: Stage) -> Connection {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest)
            .join("src/rules")
            .join(format!("{}.sql", stage.name()));
        let sql = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let conn = initialize_in_memory().unwrap();
        conn.execute_batch(&sql).unwrap();
        conn
    }

    #[test]
    fn committed_files_populate_all_stages() {
        for stage in Stage::all() {
            let conn = load_committed(stage);
            assert!(
                row_count(&conn, stage).unwrap() > 0,
                "stage {:?} should have rows after loading its committed seed",
                stage
            );
        }
    }

    #[test]
    fn prefix_seed_sort_order_is_dense() {
        // Structural invariant only: sort_order is a dense 0..N-1 sequence,
        // so loop-stage ordering round-trips. Deliberately does NOT assert
        // *which* rule is first — the rule content is editable data, not a
        // fixed source of truth.
        let conn = load_committed(Stage::Prefixes);
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
    fn seed_rows_carry_historical_timestamp() {
        // The committed seed bakes a fixed created_at so rule age is
        // meaningful and stable across re-seeds (not "now" on every load).
        let conn = load_committed(Stage::Persons);
        let distinct_created: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT created_at) FROM rule_persons",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(distinct_created, 1, "all seed rows share one timestamp");
        let ts: String = conn
            .query_row("SELECT created_at FROM rule_persons LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        // Historical, not the current year's "now"-ish default churn.
        assert!(ts.starts_with("2026-04-03"), "unexpected seed timestamp: {ts}");
    }

    #[test]
    fn dump_round_trips_identically() {
        // committed file → load → dump → reload → dump ⇒ byte-identical.
        for stage in Stage::all() {
            let conn = load_committed(stage);
            let first = dump_stage_to_string(&conn, stage).unwrap();

            let conn2 = initialize_in_memory().unwrap();
            conn2.execute_batch(&first).unwrap();
            let second = dump_stage_to_string(&conn2, stage).unwrap();

            assert_eq!(first, second, "round-trip differs for {:?}", stage);
        }
    }

    #[test]
    fn dump_reproduces_committed_files() {
        // Loading a committed file and dumping it must reproduce that
        // file byte-for-byte — guards the on-disk format against drift.
        let manifest = env!("CARGO_MANIFEST_DIR");
        for stage in Stage::all() {
            let path = std::path::Path::new(manifest)
                .join("src/rules")
                .join(format!("{}.sql", stage.name()));
            let committed = std::fs::read_to_string(&path).unwrap();
            let conn = load_committed(stage);
            let dumped = dump_stage_to_string(&conn, stage).unwrap();
            assert_eq!(
                committed, dumped,
                "committed seed for {:?} drifted from dump format \u{2014} \
                 re-run `cargo run --bin dump`",
                stage
            );
        }
    }

    #[test]
    fn load_into_db_is_noop_when_tables_populated() {
        let conn = load_committed(Stage::Merchants);
        let before: i64 = row_count(&conn, Stage::Merchants).unwrap();
        // Tables are populated, so load_into_db must not touch them.
        load_into_db(&conn).unwrap();
        let after: i64 = row_count(&conn, Stage::Merchants).unwrap();
        assert_eq!(before, after);
    }
}
