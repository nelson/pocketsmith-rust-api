pub(crate) mod banking_ops;
pub mod apply;
pub mod cache;
pub(crate) mod employers;
pub(crate) mod expand;
pub(crate) mod locations;
pub(crate) mod merchants;
pub(crate) mod persons;
pub(crate) mod prefix;
pub mod scan;
pub(crate) mod suffix;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankingOperation {
    Interest,
    CreditCard,
    Transfer,
    AccountServicing,
    Loan,
    Deposit,
    Withdrawal,
    DirectDebit,
    DirectCredit,
    BPay,
    InternalTransfer,
    Fee,
    Purchase,
    Refund,
    Cash,
}

impl BankingOperation {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Interest => "Interest",
            Self::CreditCard => "Credit Card",
            Self::Transfer => "Transfer",
            Self::AccountServicing => "Account Servicing",
            Self::Loan => "Loan",
            Self::Deposit => "Deposit",
            Self::Withdrawal => "Withdrawal",
            Self::DirectDebit => "Direct Debit",
            Self::DirectCredit => "Direct Credit",
            Self::BPay => "BPay",
            Self::InternalTransfer => "Internal Transfer",
            Self::Fee => "Fee",
            Self::Purchase => "Purchase",
            Self::Refund => "Refund",
            Self::Cash => "Cash",
        }
    }

    /// Inverse of [`display_name`](Self::display_name): map a stored
    /// `operation` string (as kept in the rule tables) back to the enum.
    /// Returns `None` for an unrecognised name.
    pub fn from_display_name(s: &str) -> Option<Self> {
        Some(match s {
            "Interest" => Self::Interest,
            "Credit Card" => Self::CreditCard,
            "Transfer" => Self::Transfer,
            "Account Servicing" => Self::AccountServicing,
            "Loan" => Self::Loan,
            "Deposit" => Self::Deposit,
            "Withdrawal" => Self::Withdrawal,
            "Direct Debit" => Self::DirectDebit,
            "Direct Credit" => Self::DirectCredit,
            "BPay" => Self::BPay,
            "Internal Transfer" => Self::InternalTransfer,
            "Fee" => Self::Fee,
            "Purchase" => Self::Purchase,
            "Refund" => Self::Refund,
            "Cash" => Self::Cash,
            _ => return None,
        })
    }
}

/// Listed in order of priority for classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayeeClass {
    Person,
    Employer,
    Merchant,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct Features {
    pub entity_name: Option<String>,
    pub location: Option<String>,
    pub region: Option<String>,
    pub operation: Option<BankingOperation>,
    pub reason: Option<String>,
    pub institution: Option<String>,
    pub gateway: Option<String>,
    pub account: Option<String>, // e.g. last 4 digits of card
    pub date: Option<String>,
    pub currency_code: Option<String>,
    pub amount_in_cents: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct NormalisationResult {
    original: String,
    pub normalised: String,
    class: Option<PayeeClass>,
    pub features: Features,
    /// Per-stage transformation log. One entry per pipeline stage that
    /// changed `normalised` or attached a feature. Populated only by
    /// [`normalise`] (raw stages don't write this).
    pub trace: Vec<TraceEntry>,
    /// Scratch slot for the current stage to record the pattern it
    /// matched. `run_traced` reads it after the stage runs, copies it
    /// into the appended [`TraceEntry`], and clears it. Stages that
    /// don't set it leave it `None`.
    #[doc(hidden)]
    pub last_matched_pattern: Option<&'static str>,
}

/// One entry in [`NormalisationResult::trace`]. Records what a single
/// pipeline stage saw on the way in, what it produced on the way out,
/// and any features it attached.
#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub stage: &'static str,
    pub before: String,
    pub after: String,
    /// New feature keys this stage populated (entity_name, location,
    /// operation, etc.). Empty if the stage only mutated the string.
    pub features_added: Vec<&'static str>,
    /// Snapshot of the values populated for each `features_added` key,
    /// captured right after the stage ran. Empty when `features_added`
    /// is empty. Held as `String` for uniform rendering even though
    /// the underlying field types vary (e.g. operation is an enum).
    pub feature_values: Vec<(&'static str, String)>,
    /// Class set by this stage, if any.
    pub class_set: Option<PayeeClass>,
    /// The pattern (regex source string) that the stage matched, if
    /// it's meaningful. Populated by table-driven stages (merchants,
    /// banking_ops, persons, employers) that try one of many patterns
    /// until one wins. `None` for stages that don't have a single
    /// matched-pattern concept (prefix/suffix/expand apply many rules).
    pub matched_pattern: Option<&'static str>,
}

impl NormalisationResult {
    pub fn new(payee: &str) -> Self {
        Self {
            original: payee.to_string(),
            normalised: payee.to_string(),
            class: None,
            features: Features::default(),
            trace: Vec::new(),
            last_matched_pattern: None,
        }
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn class(&self) -> Option<&PayeeClass> {
        self.class.as_ref()
    }

    pub fn set_class(&mut self, class: PayeeClass) {
        if self.class.is_some() {
            panic!("class already set");
        }
        self.class = Some(class);
    }
}

/// Format a normalised result into the payee string that should be written
/// to `transactions.payee`. Every classified payee is rendered from its
/// canonical `entity_name`; merchants additionally append their place as
/// `"{entity_name}, {location} {region}"` (location and/or region, whichever
/// are present). When no entity was identified we fall back to the
/// normalised string.
pub fn format_payee(result: &NormalisationResult) -> String {
    // Prefer the canonical entity name for any classified payee; fall back
    // to the normalised string when no entity was identified.
    let base = result
        .features
        .entity_name
        .clone()
        .unwrap_or_else(|| result.normalised.clone());

    // Merchants append their place: location then region, space-joined.
    if result.class() == Some(&PayeeClass::Merchant) && result.features.entity_name.is_some() {
        let place = [
            result.features.location.as_deref(),
            result.features.region.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if !place.is_empty() {
            return format!("{base}, {place}");
        }
    }
    base
}

/// Stable string tag for a [`PayeeClass`], used in DB columns and URL filters.
pub fn class_tag(class: Option<&PayeeClass>) -> Option<&'static str> {
    match class {
        Some(PayeeClass::Merchant) => Some("merchant"),
        Some(PayeeClass::Person) => Some("person"),
        Some(PayeeClass::Employer) => Some("employer"),
        Some(PayeeClass::Other) => Some("other"),
        None => None,
    }
}

/// One [`Features`] field's storage key plus its two serialisations:
/// `display` for the pipeline trace and `json` for `features_json`.
/// Both are `Some` exactly when the field is set.
struct FeatureEntry {
    key: &'static str,
    display: Option<String>,
    json: Option<serde_json::Value>,
}

/// Canonical enumeration of [`Features`] fields, in declaration order.
/// This is the single source of truth for the field list: it drives
/// [`features_to_json`], the populated-key snapshots, and the per-key
/// trace values in [`run_traced`]. Add a field here once and all three
/// pick it up.
fn feature_entries(f: &Features) -> Vec<FeatureEntry> {
    use serde_json::Value;
    fn text(key: &'static str, v: &Option<String>) -> FeatureEntry {
        FeatureEntry {
            key,
            display: v.clone(),
            json: v.clone().map(Value::String),
        }
    }
    vec![
        text("entity_name", &f.entity_name),
        text("location", &f.location),
        text("region", &f.region),
        FeatureEntry {
            key: "operation",
            display: f.operation.as_ref().map(|o| o.display_name().to_string()),
            json: f.operation.as_ref().map(|o| Value::String(o.display_name().into())),
        },
        text("reason", &f.reason),
        text("institution", &f.institution),
        text("gateway", &f.gateway),
        text("account", &f.account),
        text("date", &f.date),
        text("currency_code", &f.currency_code),
        FeatureEntry {
            key: "amount_in_cents",
            display: f.amount_in_cents.map(|c| format!("{c}c")),
            json: f.amount_in_cents.map(|c| Value::Number(c.into())),
        },
    ]
}

/// Serialise [`Features`] to a compact JSON string suitable for storage in
/// `payee_normalisations.features_json`. Only set fields are included.
pub fn features_to_json(f: &Features) -> String {
    let map: serde_json::Map<String, serde_json::Value> = feature_entries(f)
        .into_iter()
        .filter_map(|e| e.json.map(|j| (e.key.to_string(), j)))
        .collect();
    serde_json::Value::Object(map).to_string()
}

pub use cache::{OwnedPipeline, PipelineCtx, RuleCache};

/// Run the full normalisation pipeline on a raw payee string.
///
/// `ctx` bundles the DB connection + compiled-rule cache (editable-rules
/// v3 §8). In PR 2 the stages still read their in-code constants, so
/// `ctx` is threaded but not yet consulted; each conversion PR (4–8)
/// flips one stage to read from the DB via `ctx`.
pub fn normalise(original: &str, ctx: &PipelineCtx) -> NormalisationResult {
    let mut result = NormalisationResult::new(original);
    run_traced(&mut result, "prefix", |r| prefix::apply_with_db(r, ctx));
    run_traced(&mut result, "suffix", |r| suffix::apply_with_db(r, ctx));
    run_traced(&mut result, "expand", |r| expand::apply_with_db(r, ctx));
    run_traced(&mut result, "locations", |r| locations::apply_with_db(r, ctx));
    run_traced(&mut result, "persons", |r| persons::apply_with_db(r, ctx));
    run_traced(&mut result, "employers", |r| employers::apply_with_db(r, ctx));
    run_traced(&mut result, "merchants", |r| merchants::apply_with_db(r, ctx));
    run_traced(&mut result, "banking_ops", banking_ops::apply);
    // If normalised string is empty after stripping, use banking op name or "Cash"
    if result.normalised.trim().is_empty() {
        let before = result.normalised.clone();
        result.normalised = match &result.features.operation {
            Some(op) => op.display_name().to_string(),
            None => BankingOperation::Cash.display_name().to_string(),
        };
        result.trace.push(TraceEntry {
            stage: "empty-fallback",
            before,
            after: result.normalised.clone(),
            features_added: Vec::new(),
            feature_values: Vec::new(),
            class_set: None,
            matched_pattern: None,
        });
    }
    result
}

/// Snapshot the result, run a pipeline stage, and (if anything changed)
/// append a [`TraceEntry`]. Stages that have no effect produce no entry.
fn run_traced(
    result: &mut NormalisationResult,
    stage: &'static str,
    mut apply: impl FnMut(&mut NormalisationResult),
) {
    let before_str = result.normalised.clone();
    let before_keys = populated_feature_keys(&result.features);
    let before_class = result.class.clone();
    result.last_matched_pattern = None;
    apply(result);
    // Entries the stage newly populated, each carrying its own display
    // string, so the added keys and their trace values come from one
    // pass over the canonical field list.
    let feature_values: Vec<(&'static str, String)> = feature_entries(&result.features)
        .into_iter()
        .filter_map(|e| match e.display {
            Some(d) if !before_keys.contains(&e.key) => Some((e.key, d)),
            _ => None,
        })
        .collect();
    let features_added: Vec<&'static str> = feature_values.iter().map(|(k, _)| *k).collect();
    let class_set = if before_class.is_none() && result.class.is_some() {
        result.class.clone()
    } else {
        None
    };
    let matched_pattern = result.last_matched_pattern.take();
    if before_str != result.normalised || !features_added.is_empty() || class_set.is_some() {
        result.trace.push(TraceEntry {
            stage,
            before: before_str,
            after: result.normalised.clone(),
            features_added,
            feature_values,
            class_set,
            matched_pattern,
        });
    }
}

/// Names of the [`Features`] fields that are currently populated. Order
/// matches the field order in [`Features`] for deterministic output.
fn populated_feature_keys(f: &Features) -> Vec<&'static str> {
    feature_entries(f)
        .into_iter()
        .filter(|e| e.display.is_some())
        .map(|e| e.key)
        .collect()
}

/// 16-char lowercase hex of XXH3-64 hash of `original_payee`. Stable across
/// Rust versions (xxhash spec). Used as the URL slug for the review UI and
/// stored in `payee_normalisations.slug`.
pub fn slug_for(original_payee: &str) -> String {
    let h = xxhash_rust::xxh3::xxh3_64(original_payee.as_bytes());
    format!("{:016x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coverage report (`cargo test --features fidelity`): the new location
    /// stage must extract a *suburb* into `features.location` for a large
    /// share of real payees — where the old pipeline extracted the suburb in
    /// **zero** cases (it only ever recorded the trailing state code, which
    /// now lives in `features.region`). Skipped silently if the DB is absent.
    #[cfg(feature = "fidelity")]
    #[test]
    fn location_extraction_coverage_on_real_payees() {
        let Ok(conn) = rusqlite::Connection::open("pocketsmith.db") else {
            eprintln!("pocketsmith.db absent — skipping coverage test");
            return;
        };
        let payees: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT DISTINCT original_payee FROM transactions WHERE original_payee IS NOT NULL")
                .unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        let p = OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        let (mut with_location, mut with_region, mut state_code_in_location) = (0usize, 0usize, 0usize);
        for payee in &payees {
            let r = normalise(payee, &ctx);
            if let Some(loc) = &r.features.location {
                with_location += 1;
                // location must be a suburb, never a bare state/country code.
                if matches!(loc.to_uppercase().as_str(), "NSW" | "VIC" | "QLD" | "AU" | "NT" | "SA" | "WA" | "TAS" | "ACT") {
                    state_code_in_location += 1;
                }
            }
            if r.features.region.is_some() {
                with_region += 1;
            }
        }
        eprintln!(
            "location coverage: {with_location} payees have a suburb location; {with_region} have a region; {state_code_in_location} location values are bare state codes"
        );
        // The whole point of the stage: suburbs are now captured at scale.
        assert!(with_location > 4000, "expected >4000 suburb locations, got {with_location}");
        // Regions still populated (by suffix), and never leak into location.
        assert!(with_region > 2000, "expected >2000 region values, got {with_region}");
        assert_eq!(state_code_in_location, 0, "location must hold suburbs, not state codes");
    }

    /// Heavy fidelity gate (`cargo test --features fidelity`): for every
    /// distinct `original_payee` in the real `pocketsmith.db`, the
    /// DB-backed prefix+suffix stages must produce byte-identical output
    /// to the const oracle. Skipped silently if the DB isn't present.
    #[cfg(feature = "fidelity")]
    #[test]
    fn converted_stages_db_matches_const_on_real_payees() {
        let Ok(conn) = rusqlite::Connection::open("pocketsmith.db") else {
            eprintln!("pocketsmith.db absent \u{2014} skipping fidelity test");
            return;
        };
        let payees: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT DISTINCT original_payee FROM transactions WHERE original_payee IS NOT NULL")
                .unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        let p = OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        let mut checked = 0usize;
        for payee in &payees {
            for stage in ["prefix", "suffix", "expand", "persons", "employers", "merchants"] {
                let mut a = NormalisationResult::new(payee);
                let mut b = NormalisationResult::new(payee);
                match stage {
                    "prefix" => {
                        prefix::apply(&mut a);
                        prefix::apply_with_db(&mut b, &ctx);
                    }
                    "suffix" => {
                        suffix::apply(&mut a);
                        suffix::apply_with_db(&mut b, &ctx);
                    }
                    "expand" => {
                        expand::apply(&mut a);
                        expand::apply_with_db(&mut b, &ctx);
                    }
                    "persons" => {
                        persons::apply(&mut a);
                        persons::apply_with_db(&mut b, &ctx);
                    }
                    "employers" => {
                        employers::apply(&mut a);
                        employers::apply_with_db(&mut b, &ctx);
                    }
                    _ => {
                        merchants::apply(&mut a);
                        merchants::apply_with_db(&mut b, &ctx);
                    }
                }
                assert_eq!(a.normalised, b.normalised, "{stage} normalised differs for {payee:?}");
                assert_eq!(
                    features_to_json(&a.features),
                    features_to_json(&b.features),
                    "{stage} features differ for {payee:?}"
                );
                checked += 1;
            }
        }
        eprintln!("fidelity: checked {} payee\u{00d7}stage pairs", checked);
    }

    /// Conversion test — **hermetic**. Defines its own prefix rule in the
    /// DB (nothing to do with the production `src/rules/*.sql`) and proves
    /// `apply_with_db` loads + compiles + applies + captures + strips from
    /// exactly the DB rows. Tests the conversion *machinery* without
    /// treating the current rules as an oracle or source of truth. This is
    /// the template for the remaining per-stage conversions (PRs 5–8).
    #[test]
    fn prefix_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap(); // schema only
        conn.execute(
            "INSERT INTO rule_prefixes (pattern, has_account, has_date, sort_order) \
             VALUES (?1, 1, 0, 0)",
            [r"^ACCT (?P<account>\d+) "],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("ACCT 4242 SOME SHOP");
        prefix::apply_with_db(&mut r, &ctx);
        assert_eq!(r.normalised, "SOME SHOP", "prefix must be stripped using the DB rule");
        assert_eq!(r.features.account.as_deref(), Some("4242"), "named capture must be extracted");

        // No rules => no-op (the corner case db::open_app_db prevents for
        // CLIs: an unseeded DB must not panic or fabricate output).
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r2 = NormalisationResult::new("ACCT 4242 SOME SHOP");
        prefix::apply_with_db(&mut r2, &ctx2);
        assert_eq!(r2.normalised, "ACCT 4242 SOME SHOP", "no rules => input unchanged");
        assert_eq!(r2.features.account, None, "no rules => no extraction");
    }

    /// Conversion test — **hermetic** (suffix). Mirror of
    /// [`prefix_stage_reads_its_rules_from_the_db`] for the suffix stage.
    #[test]
    fn suffix_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_suffixes (pattern, has_account, sort_order) VALUES (?1, 1, 0)",
            [r"\s+CARD (?P<account>\d+)$"],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("SOME SHOP CARD 9999");
        suffix::apply_with_db(&mut r, &ctx);
        assert_eq!(r.normalised, "SOME SHOP", "suffix must be stripped using the DB rule");
        assert_eq!(r.features.account.as_deref(), Some("9999"));
    }

    /// Conversion test — **hermetic** (expand). Mirror of
    /// [`prefix_stage_reads_its_rules_from_the_db`] for the expand stage.
    /// Defines its own expansion rules in the DB (unrelated to the
    /// production `src/rules/expansions.sql`) and proves `apply_with_db`
    /// loads + compiles + word-boundary-replaces from exactly the DB rows,
    /// applying multiple rules in one pass via the expand loop.
    #[test]
    fn expand_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap(); // schema only
        // Two independent rules exercise the multi-rule loop in one call.
        conn.execute(
            "INSERT INTO rule_expansions (pattern, canonical, sort_order) VALUES (?1, ?2, 0)",
            ["WLWRTHS", "WOOLWORTHS"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO rule_expansions (pattern, canonical, sort_order) VALUES (?1, ?2, 1)",
            ["MKT", "MARKET"],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("WLWRTHS MKT");
        expand::apply_with_db(&mut r, &ctx);
        assert_eq!(
            r.normalised, "WOOLWORTHS MARKET",
            "both DB expansions must be applied in one pass"
        );
        // Word-boundary anchored: a pattern embedded in a larger word is
        // left untouched.
        let mut r2 = NormalisationResult::new("WLWRTHSX");
        expand::apply_with_db(&mut r2, &ctx);
        assert_eq!(r2.normalised, "WLWRTHSX", "no \\b match => unchanged");

        // No rules => no-op (unseeded DB must not panic or fabricate).
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r3 = NormalisationResult::new("WLWRTHS MKT");
        expand::apply_with_db(&mut r3, &ctx2);
        assert_eq!(r3.normalised, "WLWRTHS MKT", "no rules => input unchanged");
    }

    /// Conversion test — **hermetic** (persons). Defines its own person
    /// rules in the DB and proves `apply_with_db` matches case-insensitively,
    /// tags `entity_name` + `Person` class, and honours first-match-wins in
    /// `id` (declaration) order — the specific rule, inserted first, beats
    /// the generic fallback. Independent of production content.
    #[test]
    fn persons_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap(); // schema only
        // Specific rule first (lower id) so it wins over the generic one.
        conn.execute(
            "INSERT INTO rule_persons (canonical, pattern) VALUES ('Jane Cricket', 'JANE CRICKET')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO rule_persons (canonical, pattern) VALUES ('Generic Cricket', 'CRICKET')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("payment jane cricket");
        persons::apply_with_db(&mut r, &ctx);
        assert_eq!(
            r.features.entity_name.as_deref(),
            Some("Jane Cricket"),
            "case-insensitive match must pick the specific (first-id) rule"
        );
        assert_eq!(r.class(), Some(&PayeeClass::Person));

        // No rules => no match (unseeded DB must not panic or fabricate).
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r2 = NormalisationResult::new("payment jane cricket");
        persons::apply_with_db(&mut r2, &ctx2);
        assert_eq!(r2.features.entity_name, None);
        assert_eq!(r2.class(), None, "no rules => unclassified");
    }

    /// Conversion test — **hermetic** (employers). Proves `apply_with_db`
    /// compiles the DB regex, tags `entity_name` + `Employer` class, and
    /// respects the "skip if already classified" guard.
    #[test]
    fn employers_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_employers (canonical, pattern) VALUES ('Acme Corp', '(?i)\\bACME\\b')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("ACME PAYROLL");
        employers::apply_with_db(&mut r, &ctx);
        assert_eq!(r.features.entity_name.as_deref(), Some("Acme Corp"));
        assert_eq!(r.class(), Some(&PayeeClass::Employer));

        // Guard: an already-classified result is left untouched.
        let mut pre = NormalisationResult::new("ACME PAYROLL");
        pre.set_class(PayeeClass::Person);
        employers::apply_with_db(&mut pre, &ctx);
        assert_eq!(
            pre.class(),
            Some(&PayeeClass::Person),
            "must not override an existing class"
        );
        assert_eq!(pre.features.entity_name, None);

        // No rules => no match.
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r2 = NormalisationResult::new("ACME PAYROLL");
        employers::apply_with_db(&mut r2, &ctx2);
        assert_eq!(r2.class(), None, "no rules => unclassified");
    }

    /// Conversion test — **hermetic** (merchants). Mirror of the employers
    /// template for the merchants stage.
    #[test]
    fn merchants_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO rule_merchants (canonical, pattern) VALUES ('Zebra Cafe', '(?i)\\bZEBRA CAFE\\b')",
            [],
        )
        .unwrap();
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);
        let mut r = NormalisationResult::new("ZEBRA CAFE SYDNEY");
        merchants::apply_with_db(&mut r, &ctx);
        assert_eq!(r.features.entity_name.as_deref(), Some("Zebra Cafe"));
        assert_eq!(r.class(), Some(&PayeeClass::Merchant));

        // Guard: already-classified => untouched.
        let mut pre = NormalisationResult::new("ZEBRA CAFE SYDNEY");
        pre.set_class(PayeeClass::Person);
        merchants::apply_with_db(&mut pre, &ctx);
        assert_eq!(
            pre.class(),
            Some(&PayeeClass::Person),
            "must not override an existing class"
        );

        // No rules => no match.
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r2 = NormalisationResult::new("ZEBRA CAFE SYDNEY");
        merchants::apply_with_db(&mut r2, &ctx2);
        assert_eq!(r2.class(), None, "no rules => unclassified");
    }

    /// Conversion test — **hermetic** (locations). The location stage scans
    /// the *whole* normalised string (not just the tail) and records the
    /// suburb in `features.location`, additively. Defines its own
    /// `rule_locations` rows, independent of production content.
    #[test]
    fn locations_stage_reads_its_rules_from_the_db() {
        let conn = crate::db::initialize_in_memory().unwrap(); // schema only
        for loc in ["STRATHFIELD", "NORTH STRATHFIELD", "ULTIMO", "SYDNEY"] {
            conn.execute("INSERT INTO rule_locations (location) VALUES (?1)", [loc])
                .unwrap();
        }
        let cache = cache::RuleCache::new();
        let ctx = cache::PipelineCtx::new(&conn, &cache);

        // Mid-string suburb the suffix stage structurally can't reach.
        let mut r = NormalisationResult::new("GREENWAY MEAT In STRATHFIELD Date 05 Jul");
        locations::apply_with_db(&mut r, &ctx);
        assert_eq!(r.features.location.as_deref(), Some("Strathfield"));
        // Additive: the normalised string is untouched.
        assert_eq!(r.normalised, "GREENWAY MEAT In STRATHFIELD Date 05 Jul");

        // Longest match wins (NORTH STRATHFIELD over STRATHFIELD).
        let mut r2 = NormalisationResult::new("SHOP NORTH STRATHFIELD");
        locations::apply_with_db(&mut r2, &ctx);
        assert_eq!(r2.features.location.as_deref(), Some("North Strathfield"));

        // No rules => no-op (unseeded DB must not panic or fabricate).
        let bare = crate::db::initialize_in_memory().unwrap();
        let cache2 = cache::RuleCache::new();
        let ctx2 = cache::PipelineCtx::new(&bare, &cache2);
        let mut r3 = NormalisationResult::new("GREENWAY MEAT In STRATHFIELD");
        locations::apply_with_db(&mut r3, &ctx2);
        assert_eq!(r3.features.location, None, "no rules => no extraction");
    }

    /// Guards the init consolidation (not the seed *content*): every binary
    /// must open the DB via `db::open_app_db`, which seeds the rule tables,
    /// where bare `db::initialize` does not. This is the regression guard
    /// for the fresh-DB corner case where a CLI would otherwise run the
    /// pipeline against empty rule tables.
    #[test]
    fn open_app_db_seeds_rules_where_initialize_does_not() {
        let bare = crate::db::initialize_in_memory().unwrap();
        assert_eq!(
            crate::rules::count(&bare, crate::rules::Stage::Prefixes).unwrap(),
            0,
            "bare initialize must not seed (schema only)"
        );
        let dir = std::env::temp_dir().join(format!("ps-openapp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dbp = dir.join("app.db");
        let app = crate::db::open_app_db_at(dbp.to_str().unwrap()).unwrap();
        assert!(
            crate::rules::count(&app, crate::rules::Stage::Prefixes).unwrap() > 0,
            "open_app_db must seed the rule tables on a fresh DB"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_features_default() {
        let f = Features::default();
        assert!(f.entity_name.is_none());
        assert!(f.location.is_none());
        assert!(f.operation.is_none());
        assert!(f.date.is_none());
        assert!(f.currency_code.is_none());
        assert!(f.amount_in_cents.is_none());
    }

    #[test]
    fn test_payee_class_equality() {
        assert_eq!(PayeeClass::Person, PayeeClass::Person);
        assert_ne!(PayeeClass::Person, PayeeClass::Merchant);
    }

    #[test]
    fn test_banking_operation_variants() {
        assert_eq!(BankingOperation::Transfer, BankingOperation::Transfer);
        assert_ne!(BankingOperation::Transfer, BankingOperation::Interest);
    }

    #[test]
    fn test_normalisation_result_new() {
        let result = NormalisationResult::new("TEST");
        assert_eq!(result.original(), "TEST");
        assert_eq!(result.normalised, "TEST");
        assert!(result.class().is_none());
        assert!(result.features.entity_name.is_none());
        assert!(result.features.location.is_none());
    }

    #[test]
    #[should_panic(expected = "class already set")]
    fn test_set_class_twice_panics() {
        let mut r = NormalisationResult::new("TEST");
        r.set_class(PayeeClass::Person);
        r.set_class(PayeeClass::Merchant);
    }

    // --- Expand truncations tests ---

    #[test]
    fn test_expand_strathfield() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHFIELD");
    }

    #[test]
    fn test_expand_burwood() {
        let mut r = NormalisationResult::new("COLES BURWOO");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "COLES BURWOOD");
    }

    #[test]
    fn test_expand_pharmacy() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
    }

    #[test]
    fn test_expand_no_partial_match() {
        let mut r = NormalisationResult::new("STRATEGIC PLAN");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "STRATEGIC PLAN");
    }

    #[test]
    fn test_expand_multiple() {
        let mut r = NormalisationResult::new("PHARMCY BURWOO");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "PHARMACY BURWOOD");
    }

    #[test]
    fn test_expand_north_strathfield() {
        let mut r = NormalisationResult::new("SHOP NORTH STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "SHOP NORTH STRATHFIELD");
    }

    #[test]
    fn test_expand_location_suburb() {
        let mut r = NormalisationResult::new("SHOP STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "SHOP STRATHFIELD");
    }

    #[test]
    fn test_expand_location_word() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
        assert!(r.features.location.is_none());
    }

    // --- normalise() integration tests ---

    #[test]
    fn test_normalise_woolworths_full() {
        let p = OwnedPipeline::seeded_in_memory().unwrap();
        let result = normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026", &p.ctx());
        assert_eq!(result.class(), Some(&PayeeClass::Merchant));
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
    }

    #[test]
    fn test_normalise_direct_debit_comminsure() {
        let p = OwnedPipeline::seeded_in_memory().unwrap();
        let result = normalise("Direct Debit 062246 CommInsure 3791272--147492387", &p.ctx());
        assert_eq!(result.features.entity_name.as_deref(), Some("CommInsure"));
        assert_eq!(result.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(result.features.account.as_deref(), Some("062246"));
        assert_eq!(result.class(), Some(&PayeeClass::Merchant));
    }

    #[test]
    fn test_normalise_bpay() {
        let p = OwnedPipeline::seeded_in_memory().unwrap();
        let result = normalise("BPAY PAYMENT", &p.ctx());
        assert_eq!(result.class(), Some(&PayeeClass::Other));
        assert_eq!(result.features.operation, Some(BankingOperation::BPay));
    }

    // --- format_payee (moved from bin/normalise.rs) ---

    #[test]
    fn test_format_payee_merchant_with_both() {
        let mut result = NormalisationResult::new("WOOLWORTHS STRATHFIELD");
        result.normalised = "WOOLWORTHS STRATHFIELD".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Woolworths".into());
        result.features.location = Some("Strathfield".into());
        assert_eq!(format_payee(&result), "Woolworths, Strathfield");
    }

    #[test]
    fn test_format_payee_merchant_location_and_region() {
        let mut result = NormalisationResult::new("WOOLWORTHS STRATHFIELD NSW");
        result.normalised = "WOOLWORTHS".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Woolworths".into());
        result.features.location = Some("Strathfield".into());
        result.features.region = Some("NSW 2140".into());
        // "{entity}, {location} {region}"
        assert_eq!(format_payee(&result), "Woolworths, Strathfield NSW 2140");
    }

    #[test]
    fn test_format_payee_merchant_region_only() {
        let mut result = NormalisationResult::new("MERCHANT NSW");
        result.normalised = "MERCHANT".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Some Merchant".into());
        result.features.region = Some("NSW".into());
        // No suburb, region only: "{entity}, {region}"
        assert_eq!(format_payee(&result), "Some Merchant, NSW");
    }

    #[test]
    fn test_format_payee_merchant_entity_only() {
        let mut result = NormalisationResult::new("VODAFONE");
        result.normalised = "VODAFONE".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Vodafone Australia".into());
        assert_eq!(format_payee(&result), "Vodafone Australia");
    }

    #[test]
    fn test_format_payee_merchant_no_entity() {
        let mut result = NormalisationResult::new("SOME MERCHANT");
        result.normalised = "Some Merchant".into();
        result.set_class(PayeeClass::Merchant);
        assert_eq!(format_payee(&result), "Some Merchant");
    }

    #[test]
    fn test_format_payee_person() {
        let mut result = NormalisationResult::new("MR JOHN SMITH");
        result.normalised = "MR JOHN SMITH".into();
        result.set_class(PayeeClass::Person);
        result.features.entity_name = Some("John Smith".into());
        // Persons render from the canonical entity name, not the
        // normalised string, and never append a location.
        result.features.location = Some("Strathfield".into());
        assert_eq!(format_payee(&result), "John Smith");
    }

    #[test]
    fn test_format_payee_employer() {
        let mut result = NormalisationResult::new("SALARY FROM ACME");
        result.normalised = "SALARY FROM ACME".into();
        result.set_class(PayeeClass::Employer);
        result.features.entity_name = Some("Acme Corp".into());
        assert_eq!(format_payee(&result), "Acme Corp");
    }

    #[test]
    fn test_format_payee_unclassified() {
        let result = NormalisationResult::new("UNKNOWN");
        assert_eq!(format_payee(&result), "UNKNOWN");
    }

    #[test]
    fn test_class_tag() {
        assert_eq!(class_tag(Some(&PayeeClass::Merchant)), Some("merchant"));
        assert_eq!(class_tag(Some(&PayeeClass::Person)), Some("person"));
        assert_eq!(class_tag(Some(&PayeeClass::Employer)), Some("employer"));
        assert_eq!(class_tag(Some(&PayeeClass::Other)), Some("other"));
        assert_eq!(class_tag(None), None);
    }

    #[test]
    fn test_features_to_json_empty() {
        let f = Features::default();
        assert_eq!(features_to_json(&f), "{}");
    }

    #[test]
    fn test_features_to_json_with_fields() {
        let mut f = Features::default();
        f.entity_name = Some("Woolworths".into());
        f.location = Some("Strathfield".into());
        f.operation = Some(BankingOperation::DirectDebit);
        f.amount_in_cents = Some(1234);
        let s = features_to_json(&f);
        // Order isn't guaranteed across serde versions, so just check it parses
        // and contains the expected keys.
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["entity_name"], "Woolworths");
        assert_eq!(v["location"], "Strathfield");
        assert_eq!(v["operation"], "Direct Debit");
        assert_eq!(v["amount_in_cents"], 1234);
    }
}
