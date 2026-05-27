mod banking_ops;
pub mod apply;
mod employers;
mod expand;
#[allow(dead_code)]
mod locations;
mod merchants;
mod persons;
mod prefix;
pub mod scan;
mod suffix;

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
/// to `transactions.payee`. Merchant rows combine entity_name + location
/// when both are present; otherwise we fall back to the normalised string.
/// Non-merchant classes always use the normalised string verbatim.
pub fn format_payee(result: &NormalisationResult) -> String {
    match result.class() {
        Some(PayeeClass::Merchant) => match (&result.features.entity_name, &result.features.location) {
            (Some(name), Some(loc)) => format!("{} {}", name, loc),
            (Some(name), None) => name.clone(),
            _ => result.normalised.clone(),
        },
        _ => result.normalised.clone(),
    }
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

/// Serialise [`Features`] to a compact JSON string suitable for storage in
/// `payee_normalisations.features_json`. Only set fields are included.
pub fn features_to_json(f: &Features) -> String {
    let mut map = serde_json::Map::new();
    if let Some(v) = &f.entity_name { map.insert("entity_name".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.location { map.insert("location".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.operation { map.insert("operation".into(), serde_json::Value::String(v.display_name().into())); }
    if let Some(v) = &f.reason { map.insert("reason".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.institution { map.insert("institution".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.gateway { map.insert("gateway".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.account { map.insert("account".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.date { map.insert("date".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = &f.currency_code { map.insert("currency_code".into(), serde_json::Value::String(v.clone())); }
    if let Some(v) = f.amount_in_cents { map.insert("amount_in_cents".into(), serde_json::Value::Number(v.into())); }
    serde_json::Value::Object(map).to_string()
}

/// Run the full normalisation pipeline on a raw payee string.
/// Run the full normalisation pipeline on a raw payee string.
pub fn normalise(original: &str) -> NormalisationResult {
    let mut result = NormalisationResult::new(original);
    run_traced(&mut result, "prefix", prefix::apply);
    run_traced(&mut result, "suffix", suffix::apply);
    run_traced(&mut result, "expand", expand::apply);
    run_traced(&mut result, "persons", persons::apply);
    run_traced(&mut result, "employers", employers::apply);
    run_traced(&mut result, "merchants", merchants::apply);
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
    apply: fn(&mut NormalisationResult),
) {
    let before_str = result.normalised.clone();
    let before_keys = populated_feature_keys(&result.features);
    let before_class = result.class.clone();
    result.last_matched_pattern = None;
    apply(result);
    let after_keys = populated_feature_keys(&result.features);
    let features_added: Vec<&'static str> = after_keys
        .into_iter()
        .filter(|k| !before_keys.contains(k))
        .collect();
    let feature_values: Vec<(&'static str, String)> = features_added
        .iter()
        .filter_map(|k| feature_value(&result.features, k).map(|v| (*k, v)))
        .collect();
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

/// String form of a single populated feature value, suitable for the
/// pipeline-trace render. Returns `None` for keys whose value is
/// unset — keeps `feature_values` aligned with what the stage actually
/// populated.
fn feature_value(f: &Features, key: &str) -> Option<String> {
    match key {
        "entity_name" => f.entity_name.clone(),
        "location" => f.location.clone(),
        "operation" => f.operation.as_ref().map(|o| o.display_name().to_string()),
        "reason" => f.reason.clone(),
        "institution" => f.institution.clone(),
        "gateway" => f.gateway.clone(),
        "account" => f.account.clone(),
        "date" => f.date.clone(),
        "currency_code" => f.currency_code.clone(),
        "amount_in_cents" => f.amount_in_cents.map(|c| format!("{c}c")),
        _ => None,
    }
}

/// Names of the [`Features`] fields that are currently populated. Order
/// matches the field order in [`Features`] for deterministic output.
fn populated_feature_keys(f: &Features) -> Vec<&'static str> {
    let mut out = Vec::new();
    if f.entity_name.is_some() { out.push("entity_name"); }
    if f.location.is_some() { out.push("location"); }
    if f.operation.is_some() { out.push("operation"); }
    if f.reason.is_some() { out.push("reason"); }
    if f.institution.is_some() { out.push("institution"); }
    if f.gateway.is_some() { out.push("gateway"); }
    if f.account.is_some() { out.push("account"); }
    if f.date.is_some() { out.push("date"); }
    if f.currency_code.is_some() { out.push("currency_code"); }
    if f.amount_in_cents.is_some() { out.push("amount_in_cents"); }
    out
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
        let result = normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(result.class(), Some(&PayeeClass::Merchant));
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
    }

    #[test]
    fn test_normalise_direct_debit_comminsure() {
        let result = normalise("Direct Debit 062246 CommInsure 3791272--147492387");
        assert_eq!(result.features.entity_name.as_deref(), Some("CommInsure"));
        assert_eq!(result.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(result.features.account.as_deref(), Some("062246"));
        assert_eq!(result.class(), Some(&PayeeClass::Merchant));
    }

    #[test]
    fn test_normalise_bpay() {
        let result = normalise("BPAY PAYMENT");
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
        assert_eq!(format_payee(&result), "Woolworths Strathfield");
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
        let mut result = NormalisationResult::new("JOHN SMITH");
        result.normalised = "John Smith".into();
        result.set_class(PayeeClass::Person);
        assert_eq!(format_payee(&result), "John Smith");
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
