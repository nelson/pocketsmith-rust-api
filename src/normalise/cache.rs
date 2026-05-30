//! Pipeline context + per-stage rule cache (editable-rules-v3 §7, §8).
//!
//! [`normalise`](super::normalise) takes a [`PipelineCtx`] bundling the
//! database connection and a [`RuleCache`]. The connection is the source
//! of the editable rule tables; the cache holds the compiled form of
//! each stage's rules so we don't recompile regexes on every payee.
//!
//! In PR 2 this is pure plumbing: the cache has no populated slots yet
//! and the stages still read their in-code `const` dictionaries. Each
//! conversion PR (4–8) adds the slot for the stage it converts and an
//! `apply_with_db` that consults it. Threading the context through every
//! call site now means those PRs only touch the one stage they convert.

use rusqlite::Connection;

use crate::rules::Stage;

/// Process-lifetime cache of compiled rules, keyed by stage.
///
/// Per the plan (§7) there is no global generation counter: a rule edit
/// invalidates only the affected stage's slot, and the next read of that
/// stage recompiles just it. Slots are added as each stage is converted
/// to read from the DB (PR 4+); until then this is an empty skeleton.
#[derive(Default)]
pub struct RuleCache {
    // Per-stage `RwLock<Option<Arc<Vec<Compiled…>>>>` slots are added by
    // the conversion PRs. Intentionally empty for now.
}

impl RuleCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the cached compilation for one stage so the next read
    /// recompiles it from the (just-edited) DB rows. No-op while the
    /// stage has no cache slot yet.
    pub fn invalidate(&self, stage: Stage) {
        // No slots populated yet (PR 2). Match exhaustively so adding a
        // slot in a later PR forces this to be updated.
        match stage {
            Stage::Prefixes
            | Stage::Suffixes
            | Stage::Expansions
            | Stage::Persons
            | Stage::Employers
            | Stage::Merchants
            | Stage::BankingOps
            | Stage::Locations => {}
        }
    }
}

/// Everything a pipeline stage needs to evaluate rules: the DB
/// connection (source of the editable rule tables) and the compiled-rule
/// cache. Cheap to construct — it only borrows.
pub struct PipelineCtx<'a> {
    pub conn: &'a Connection,
    pub cache: &'a RuleCache,
}

impl<'a> PipelineCtx<'a> {
    pub fn new(conn: &'a Connection, cache: &'a RuleCache) -> Self {
        Self { conn, cache }
    }
}

/// Owns a [`Connection`] + [`RuleCache`] so callers (tests, the scan
/// loop, one-shot binaries) can hold the backing storage and hand out
/// borrowed [`PipelineCtx`]s. `PipelineCtx` itself only borrows, so it
/// can't own its storage.
pub struct OwnedPipeline {
    pub conn: Connection,
    pub cache: RuleCache,
}

impl OwnedPipeline {
    /// Wrap an existing connection (e.g. the serve/scan DB).
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            cache: RuleCache::new(),
        }
    }

    /// In-memory DB seeded from the canonical `src/rules/*.sql` files —
    /// for tests and fidelity checks (§8). Seeding the eight small rule
    /// tables is ~1ms.
    pub fn seeded_in_memory() -> anyhow::Result<Self> {
        let conn = crate::db::initialize_in_memory()?;
        crate::rules::load_into_db(&conn)?;
        Ok(Self::new(conn))
    }

    /// Borrow a context for a single `normalise` call.
    pub fn ctx(&self) -> PipelineCtx<'_> {
        PipelineCtx::new(&self.conn, &self.cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_is_a_noop_for_every_stage() {
        let cache = RuleCache::new();
        for stage in Stage::all() {
            cache.invalidate(stage); // must not panic
        }
    }

    #[test]
    fn seeded_in_memory_builds_a_usable_ctx() {
        let owned = OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = owned.ctx();
        // The connection is live and the rule tables are seeded.
        let n: i64 = ctx
            .conn
            .query_row("SELECT COUNT(*) FROM rule_merchants", [], |r| r.get(0))
            .unwrap();
        assert!(n > 0);
    }
}
