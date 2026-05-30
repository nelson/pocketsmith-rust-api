//! Owned, DB-shaped representations of each pipeline stage's seed rows.
//!
//! In v3 the canonical rule tables live in SQLite (and are mirrored to
//! `src/rules/*.sql`). These structs are the bridge used to *bootstrap*
//! those tables from the in-code `const` dictionaries that still drive
//! the pipeline until each stage is converted (PR 2+). Each
//! `normalise::<stage>` module exposes a `seed_rows()` accessor that
//! returns these; [`super::bootstrap_from_constants`] inserts them.
//!
//! Once every stage reads from the DB these accessors become a pure
//! fallback / fidelity oracle — they are intentionally kept as the
//! ground truth for the seed.

/// `rule_prefixes` row (loop stage — `sort_order` = declaration index).
#[derive(Debug, Clone)]
pub struct PrefixSeed {
    pub pattern: String,
    pub gateway: Option<String>,
    pub operation: Option<String>,
    pub has_account: bool,
    pub has_date: bool,
}

/// `rule_suffixes` row (loop stage — `sort_order` = declaration index).
#[derive(Debug, Clone)]
pub struct SuffixSeed {
    pub pattern: String,
    pub gateway: Option<String>,
    pub operation: Option<String>,
    pub institution: Option<String>,
    pub has_account: bool,
    pub has_date: bool,
    pub has_location: bool,
    pub has_currency_code: bool,
    pub has_amount: bool,
}

/// `rule_expansions` row (loop stage — `sort_order` = declaration index).
#[derive(Debug, Clone)]
pub struct ExpansionSeed {
    pub pattern: String,
    pub canonical: String,
}

/// `rule_persons` row. One per (canonical, pattern) pair. Declaration
/// order is preserved via the autoincrement id so a later conversion can
/// reproduce first-match-wins behaviour if needed.
#[derive(Debug, Clone)]
pub struct PersonSeed {
    pub canonical: String,
    pub pattern: String,
}

/// `rule_employers` row. One per (canonical, pattern) pair.
#[derive(Debug, Clone)]
pub struct EmployerSeed {
    pub canonical: String,
    pub pattern: String,
}

/// `rule_merchants` row. One per pattern (unique).
#[derive(Debug, Clone)]
pub struct MerchantSeed {
    pub pattern: String,
    pub canonical: String,
}

/// `rule_banking_ops` row. One per (operation, pattern) pair
/// (loop stage — `sort_order` = declaration index).
#[derive(Debug, Clone)]
pub struct BankingOpSeed {
    pub operation: String,
    pub pattern: String,
    pub has_account: bool,
}
