//! The shared commit seam (rule-cli §3.6): the single "save a rule"
//! entry point. Takes **exactly one** [`Mutation`], performs it inside
//! one `with_operation`, invalidates the cache (if supplied), and writes
//! the canonical `.sql` mirror per [`DumpPolicy`]. There is no batching —
//! one commit is one atomic change producing one `_operations` row and
//! one activity line.
//!
//! `commit` vs `dump` (review naming question): they are related but
//! different scopes.
//!   * **commit** persists *one rule change* to the DB (the authority).
//!   * **dump** writes the human-reviewable `rules/<stage>.sql` *mirror*
//!     of the DB — the same operation the `dump` binary performs for all
//!     eight stages. Committing a change re-dumps only the one stage it
//!     touched, so the mirror tracks the DB.
//!
//! At-rest consistency: the CLI commits with [`DumpPolicy::Sync`], which
//! re-dumps the stage *before the process exits*, so once `rule … --apply`
//! returns the `.sql` already matches the DB. The DB is always the
//! authority; the `.sql` is only read to re-seed an empty table on a cold
//! start, never to overwrite live rows.

use anyhow::Result;
use rusqlite::Connection;

use super::crud;
use super::model::{Mutation, Rule};
use super::{activity::RuleChange, dirty};
use crate::normalise::RuleCache;

/// *When* (and where) the canonical `rules/<stage>.sql` mirror is written
/// after a commit — purely the dump's timing, never *what* is written, so
/// it cannot change pipeline behaviour (rule-cli §1.2). Both variants
/// re-dump identical content; they differ only in scheduling.
#[derive(Debug, Clone)]
pub enum DumpPolicy {
    /// CLI: dump inline into the given directory before the process
    /// exits, so the mirror matches the DB at rest. The dir is injected
    /// (not read from a global) so callers and tests stay parallel-safe;
    /// production passes `rules::rules_dir()`.
    Sync(std::path::PathBuf),
    /// Long-running host (the future serve integration): re-dump the
    /// stage on a detached background thread so an HTTP response isn't
    /// blocked on disk I/O. **Not yet wired to serve** (serve is still
    /// read-only); currently exercised only by the parity test. When
    /// serve adopts this, it should also dump on shutdown to preserve
    /// at-rest consistency under an abrupt kill.
    Background { db_path: String },
}

/// What a commit produced: the activity line, the post-commit dirty
/// count, and (for an add) the new row id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    pub change: String,
    pub dirty_payees: usize,
    pub new_id: Option<i64>,
}

/// Commit one rule mutation atomically. Always mutates the `rule_*`
/// tables under a single `with_operation` and always invalidates `cache`
/// (if supplied); `dump` only chooses *when* the `.sql` mirror is written.
pub fn commit(
    conn: &Connection,
    mutation: &Mutation,
    dump: DumpPolicy,
    cache: Option<&RuleCache>,
) -> Result<CommitResult> {
    let stage = mutation.stage();

    // Snapshot the pre-edit row so the activity line can show old → new.
    // Every mutation except Add identifies an existing row by id.
    let before: Option<Rule> = match mutation {
        Mutation::Add(_) => None,
        Mutation::Edit { id, .. } | Mutation::Delete { id, .. } | Mutation::Move { id, .. } => {
            crud::get(conn, stage, *id)?
        }
    };

    // One operation = one CRUD call = one `_operations` row.
    let new_id = crate::db::with_operation(conn, "rule-edit", |conn| {
        Ok(match mutation {
            Mutation::Add(data) => Some(crud::insert_rule(conn, data)?),
            Mutation::Edit { id, data } => {
                crud::update_rule(conn, *id, data)?;
                None
            }
            Mutation::Delete { stage, id } => {
                crud::delete_rule(conn, *stage, *id)?;
                None
            }
            Mutation::Move { stage, id, target } => {
                crud::move_rule(conn, *stage, *id, *target)?;
                None
            }
        })
    })?;

    // The compiled cache (if any) must recompile this stage from the
    // freshly-committed rows. Identical for both dump policies.
    if let Some(cache) = cache {
        cache.invalidate(stage);
    }

    let change = RuleChange::describe(mutation, before.as_ref());
    let dirty_payees = dirty::would_restage(conn)?;

    match dump {
        DumpPolicy::Sync(dir) => super::dump_stage_to(conn, stage, &dir)?,
        DumpPolicy::Background { db_path } => super::schedule_dump(stage, db_path),
    }

    Ok(CommitResult { change, dirty_payees, new_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::model::{MoveTarget, Mutation, RuleData};
    use crate::rules::Stage;
    use crate::normalise::RuleCache;

    fn merchant(canonical: &str, pattern: &str) -> RuleData {
        RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
    }

    /// A fresh, unique temp directory for an injected `DumpPolicy::Sync`.
    /// No global state — each test owns its own dir, so tests stay parallel.
    fn tmpdir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rule-commit-{}-{:?}-{n}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn commit_add_inserts_dumps_and_returns_change() {
        let dir = tmpdir();
        let conn = initialize_in_memory().unwrap();
        let m = Mutation::Add(merchant("Bunnings", "(?i)BUNNINGS"));
        let res = commit(&conn, &m, DumpPolicy::Sync(dir.clone()), None).unwrap();
        assert_eq!(res.change, "+ added Bunnings (?i)BUNNINGS");
        let id = res.new_id.unwrap();
        assert_eq!(
            crud::get(&conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(),
            Some("(?i)BUNNINGS")
        );
        // The stage's .sql mirror was re-dumped into the injected dir.
        let f = dir.join("merchants.sql");
        assert!(f.exists());
        assert!(std::fs::read_to_string(&f).unwrap().contains("(?i)BUNNINGS"));
    }

    #[test]
    fn commit_invalidates_cache_for_both_policies() {
        // The §1.2 guarantee: cache invalidation happens regardless of dump
        // policy. Sync writes to an injected temp dir; Background's detached
        // dump targets an empty :memory: DB (it no-ops without touching any
        // real file) — neither affects the cache assertion.
        for sync in [true, false] {
            let conn = initialize_in_memory().unwrap();
            let cache = RuleCache::new();
            let m = Mutation::Add(merchant("Uber", "(?i)UBER"));
            let dump = if sync {
                DumpPolicy::Sync(tmpdir())
            } else {
                DumpPolicy::Background { db_path: ":memory:".into() }
            };
            commit(&conn, &m, dump, Some(&cache)).unwrap();
            assert!(
                cache_resolves_uber(&cache, &conn),
                "cache must recompile from committed rows"
            );
        }
    }

    /// Whether the (warm) cache resolves "UBER TRIP" to the just-committed
    /// "Uber" merchant — exercises the public pipeline read path.
    fn cache_resolves_uber(cache: &RuleCache, conn: &Connection) -> bool {
        use crate::normalise::{normalise, PipelineCtx};
        let ctx = PipelineCtx::new(conn, cache);
        let r = normalise("UBER TRIP", &ctx);
        r.features.entity_name.as_deref() == Some("Uber")
    }

    #[test]
    fn commit_move_renumbers_and_describes() {
        let conn = initialize_in_memory().unwrap();
        let a = crud::insert_rule(
            &conn,
            &RuleData::Prefix {
                pattern: "^A ".into(),
                gateway: None,
                operation: None,
                has_account: false,
                has_date: false,
                note: None,
            },
        )
        .unwrap();
        let b = crud::insert_rule(
            &conn,
            &RuleData::Prefix {
                pattern: "^B ".into(),
                gateway: None,
                operation: None,
                has_account: false,
                has_date: false,
                note: None,
            },
        )
        .unwrap();
        let m = Mutation::Move { stage: Stage::Prefixes, id: b, target: MoveTarget::Before(a) };
        let res = commit(&conn, &m, DumpPolicy::Sync(tmpdir()), None).unwrap();
        assert_eq!(res.change, format!("moved prefix #{b} before #{a}"));
        let order: Vec<i64> =
            crud::list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(order, vec![b, a]);
    }
}
