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

use std::sync::{Arc, RwLock};

use anyhow::Result;
use rusqlite::Connection;

use super::prefix::CompiledPrefix;
use super::suffix::CompiledSuffix;
use super::expand::CompiledExpansion;
use super::persons::CompiledPerson;
use super::employers::CompiledEmployer;
use super::merchants::CompiledMerchant;
use super::banking_ops::CompiledBankingOp;
use crate::rules::Stage;

/// Process-lifetime cache of compiled rules, keyed by stage.
///
/// Per the plan (§7) there is no global generation counter: a rule edit
/// invalidates only the affected stage's slot, and the next read of that
/// stage recompiles just it. A slot is added here as each stage is
/// converted to read from the DB (PR 4+).
#[derive(Default)]
pub struct RuleCache {
    prefixes: RwLock<Option<Arc<Vec<CompiledPrefix>>>>,
    suffixes: RwLock<Option<Arc<Vec<CompiledSuffix>>>>,
    expansions: RwLock<Option<Arc<Vec<CompiledExpansion>>>>,
    persons: RwLock<Option<Arc<Vec<CompiledPerson>>>>,
    employers: RwLock<Option<Arc<Vec<CompiledEmployer>>>>,
    merchants: RwLock<Option<Arc<Vec<CompiledMerchant>>>>,
    locations: RwLock<Option<Arc<super::locations::LocationRules>>>,
    banking_ops: RwLock<Option<Arc<Vec<CompiledBankingOp>>>>,
}

impl RuleCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compiled prefix rules, loading + compiling on first use and on the
    /// first read after an invalidation.
    pub(crate) fn prefixes(&self, conn: &Connection) -> Result<Arc<Vec<CompiledPrefix>>> {
        if let Some(arc) = self.prefixes.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::prefix::load_compiled(conn)?);
        *self.prefixes.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled suffix rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn suffixes(&self, conn: &Connection) -> Result<Arc<Vec<CompiledSuffix>>> {
        if let Some(arc) = self.suffixes.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::suffix::load_compiled(conn)?);
        *self.suffixes.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled expansion rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn expansions(&self, conn: &Connection) -> Result<Arc<Vec<CompiledExpansion>>> {
        if let Some(arc) = self.expansions.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::expand::load_compiled(conn)?);
        *self.expansions.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled person rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn persons(&self, conn: &Connection) -> Result<Arc<Vec<CompiledPerson>>> {
        if let Some(arc) = self.persons.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::persons::load_compiled(conn)?);
        *self.persons.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled employer rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn employers(&self, conn: &Connection) -> Result<Arc<Vec<CompiledEmployer>>> {
        if let Some(arc) = self.employers.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::employers::load_compiled(conn)?);
        *self.employers.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled merchant rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn merchants(&self, conn: &Connection) -> Result<Arc<Vec<CompiledMerchant>>> {
        if let Some(arc) = self.merchants.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::merchants::load_compiled(conn)?);
        *self.merchants.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Known places (suburbs + regions), see [`prefixes`](Self::prefixes).
    pub(crate) fn locations(&self, conn: &Connection) -> Result<Arc<super::locations::LocationRules>> {
        if let Some(arc) = self.locations.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::locations::load_compiled(conn)?);
        *self.locations.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Compiled banking-op rules (see [`prefixes`](Self::prefixes)).
    pub(crate) fn banking_ops(&self, conn: &Connection) -> Result<Arc<Vec<CompiledBankingOp>>> {
        if let Some(arc) = self.banking_ops.read().unwrap().as_ref() {
            return Ok(arc.clone());
        }
        let arc = Arc::new(super::banking_ops::load_compiled(conn)?);
        *self.banking_ops.write().unwrap() = Some(arc.clone());
        Ok(arc)
    }

    /// Drop the cached compilation for one stage so the next read
    /// recompiles it from the (just-edited) DB rows. No-op for stages
    /// not yet converted to read from the DB.
    pub fn invalidate(&self, stage: Stage) {
        match stage {
            Stage::Prefixes => *self.prefixes.write().unwrap() = None,
            Stage::Suffixes => *self.suffixes.write().unwrap() = None,
            Stage::Expansions => *self.expansions.write().unwrap() = None,
            Stage::Persons => *self.persons.write().unwrap() = None,
            Stage::Employers => *self.employers.write().unwrap() = None,
            Stage::Merchants => *self.merchants.write().unwrap() = None,
            Stage::Locations => *self.locations.write().unwrap() = None,
            Stage::BankingOps => *self.banking_ops.write().unwrap() = None,
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

    /// In-memory DB seeded from the canonical `rules/*.sql` files —
    /// the basis for the hermetic per-stage tests. Seeding the eight small
    /// rule tables is ~1ms.
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

    /// E2E editability contract (the automated form of the manual UAT):
    /// a live rule edit changes pipeline output, but **only after the
    /// affected stage's cache slot is invalidated**. Within one process and
    /// one warm [`RuleCache`] it drives the full read → compile → cache →
    /// edit → invalidate → recompile path that the 4b editor will use.
    ///
    /// Hermetic: defines its own `rule_expansions` row, independent of the
    /// production seed content. Uses a nonsense token so no downstream
    /// (still-const) stage rewrites the result.
    #[test]
    fn editing_a_rule_takes_effect_only_after_invalidate() {
        let conn = crate::db::initialize_in_memory().unwrap(); // schema only
        conn.execute(
            "INSERT INTO rule_expansions (pattern, canonical, sort_order) VALUES ('ZQX', 'FIRST', 0)",
            [],
        )
        .unwrap();
        let cache = RuleCache::new();
        let ctx = PipelineCtx::new(&conn, &cache);

        // First read compiles + caches 'ZQX' -> 'FIRST'.
        let mut a = crate::normalise::NormalisationResult::new("ZQX SHOP");
        crate::normalise::expand::apply_with_db(&mut a, &ctx);
        assert_eq!(a.normalised, "FIRST SHOP", "DB rule must drive the expansion");

        // Edit the rule in the DB underneath the warm cache.
        conn.execute(
            "UPDATE rule_expansions SET canonical = 'SECOND' WHERE pattern = 'ZQX'",
            [],
        )
        .unwrap();

        // Until invalidated, the cache must keep serving the pre-edit rule
        // (otherwise the assertion below would be vacuous).
        let mut b = crate::normalise::NormalisationResult::new("ZQX SHOP");
        crate::normalise::expand::apply_with_db(&mut b, &ctx);
        assert_eq!(
            b.normalised, "FIRST SHOP",
            "warm cache must serve the pre-edit rule until its stage is invalidated"
        );

        // Invalidating only this stage forces the next read to recompile
        // from the edited rows.
        cache.invalidate(Stage::Expansions);
        let mut c = crate::normalise::NormalisationResult::new("ZQX SHOP");
        crate::normalise::expand::apply_with_db(&mut c, &ctx);
        assert_eq!(
            c.normalised, "SECOND SHOP",
            "after invalidate the edited rule must take effect"
        );
    }

    /// Invalidating one stage must not drop another stage's cached
    /// compilation. Guards against a too-broad `invalidate` match arm.
    #[test]
    fn invalidate_is_scoped_to_one_stage() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_expansions (pattern, canonical, sort_order) VALUES ('ZQX', 'FIRST', 0)",
            [],
        )
        .unwrap();
        let cache = RuleCache::new();
        let ctx = PipelineCtx::new(&conn, &cache);

        let mut a = crate::normalise::NormalisationResult::new("ZQX SHOP");
        crate::normalise::expand::apply_with_db(&mut a, &ctx); // warm the expansions slot
        assert_eq!(a.normalised, "FIRST SHOP");

        conn.execute(
            "UPDATE rule_expansions SET canonical = 'SECOND' WHERE pattern = 'ZQX'",
            [],
        )
        .unwrap();

        // Invalidating an unrelated stage must leave the expansions slot warm.
        cache.invalidate(Stage::Prefixes);
        let mut b = crate::normalise::NormalisationResult::new("ZQX SHOP");
        crate::normalise::expand::apply_with_db(&mut b, &ctx);
        assert_eq!(
            b.normalised, "FIRST SHOP",
            "invalidating another stage must not evict the expansions cache"
        );
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
