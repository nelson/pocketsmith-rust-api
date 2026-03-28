use anyhow::Result;
use std::path::Path;

use crate::normalise::Metadata;
#[cfg(test)]
use crate::normalise::meta;
use crate::normalise::rules::{
    CompiledCleanupRules, CompiledClassifyRules, CompiledExpandRules,
    CompiledIdentifyRules, CompiledStripRules,
};
use crate::normalise::{strip, classify, expand, identify, cleanup};

/// All compiled rules for the 5-stage pipeline.
pub struct PipelineRules {
    strip: CompiledStripRules,
    classify: CompiledClassifyRules,
    expand: CompiledExpandRules,
    identify: CompiledIdentifyRules,
    cleanup: CompiledCleanupRules,
}

impl PipelineRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let expand = CompiledExpandRules::load(rules_dir)?;
        let mut identify = CompiledIdentifyRules::load(rules_dir)?;
        // Share known locations from expand stage into identify stage for capture dedup
        identify.known_locations = expand.locations.iter().map(|l| l.name.to_uppercase()).collect();
        Ok(Self {
            strip: CompiledStripRules::load(rules_dir)?,
            classify: CompiledClassifyRules::load(rules_dir)?,
            expand,
            identify,
            cleanup: CompiledCleanupRules::load(rules_dir)?,
        })
    }
}

/// Run a single payee through all 5 stages. Returns (normalised, metadata).
pub fn normalise_payee(original_payee: &str, rules: &PipelineRules) -> (String, Metadata) {
    let mut metadata = Metadata::new();

    // Stage 1: Strip prefixes and suffixes (repeat until stable, max 5)
    let mut s1_out = original_payee.to_string();
    let mut s1_reps = 1;
    for rep in 1..=5 {
        let prev = s1_out.clone();
        let (result, s1_meta) = strip::apply(&s1_out, &rules.strip);
        s1_out = result;
        // Merge stage 1 metadata (only first pass sets prefix, accumulate suffixes)
        for (k, v) in s1_meta {
            metadata.entry(k).or_insert(v);
        }
        s1_reps = rep;
        if s1_out == prev {
            break;
        }
    }
    metadata.insert("stage1_repeats".into(), serde_json::Value::Number(s1_reps.into()));

    // Stage 2: Classify type (uses original for pattern matching)
    classify::apply(original_payee, &mut metadata, &rules.classify);

    // Stage 3: Expand truncations
    let s3_out = expand::apply(&s1_out, &mut metadata, &rules.expand);

    // Stage 4: Identity-based normalisation
    let s4_out = identify::apply(&s3_out, original_payee, &mut metadata, &rules.identify);

    // Stage 5: Final cleanup (repeat until stable, max 5)
    let mut s5_out = s4_out;
    for _ in 1..=5 {
        let prev = s5_out.clone();
        s5_out = cleanup::apply(&s5_out, &mut metadata, &rules.cleanup);
        if s5_out == prev {
            break;
        }
    }

    (s5_out, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn rules() -> PipelineRules {
        PipelineRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_full_pipeline_merchant() {
        let r = rules();
        let (result, meta) = normalise_payee("SQ *SOME COFFEE SHOP Card xx1234 Value Date 01/01/2024", &r);
        assert_eq!(result, "Some Coffee Shop");
        assert_eq!(meta[meta::TYPE], "merchant");
        assert_eq!(meta[meta::PREFIX_STRIPPED], "Square");
    }

    #[test]
    fn test_full_pipeline_woolworths() {
        let r = rules();
        let (result, _meta) = normalise_payee("WOOLWORTHS 1234 STRATHFIEL", &r);
        // Expand: STRATHFIEL -> STRATHFIELD, Identify: Woolworths {1}, Cleanup: title case
        assert_eq!(result, "Woolworths Strathfield");
    }

    #[test]
    fn test_full_pipeline_salary() {
        let r = rules();
        let (result, meta) = normalise_payee("Salary from APPLE PTY LTD", &r);
        assert_eq!(result, "Apple (Salary)");
        assert_eq!(meta[meta::TYPE], "salary");
    }

    #[test]
    fn test_full_pipeline_transfer() {
        let r = rules();
        let (result, meta) = normalise_payee("Transfer to xx1234 CommBank App", &r);
        assert_eq!(result, "Internal Account Transfer");
        assert_eq!(meta[meta::TYPE], "transfer_out");
    }

    #[test]
    fn test_full_pipeline_banking() {
        let r = rules();
        let (result, meta) = normalise_payee("Direct Debit 12345 AFES", &r);
        assert_eq!(result, "AFES (Donation)");
        assert_eq!(meta[meta::TYPE], "banking_operation");
    }

    #[test]
    fn test_full_pipeline_transport_nsw() {
        let r = rules();
        let (result, _meta) = normalise_payee("TRANSPORTFORNSWTRAVEL SYDNEY NSW", &r);
        assert_eq!(result, "Transport NSW");
    }

    #[test]
    fn test_full_pipeline_osko_person() {
        let r = rules();
        let (result, meta) = normalise_payee("Lok Sun Nelson Tam 12345678 - Osko Payment - Receipt 12345", &r);
        assert_eq!(result, "Nelson Tam");
        assert_eq!(meta[meta::TYPE], "transfer_in");
    }

    #[test]
    fn test_full_pipeline_strip_repeat() {
        let r = rules();
        let (result, _meta) = normalise_payee("ACME PTY LTD NSW Card xx1234 Value Date 01/01/2024", &r);
        // Strip removes suffix first, then PTY LTD on second pass
        assert_eq!(result, "Acme");
    }

    #[test]
    fn test_full_pipeline_amazon() {
        let r = rules();
        let (result, _meta) = normalise_payee("AMAZON MKTPL*XY12345 Card xx1234 Value Date 01/01/2024", &r);
        assert_eq!(result, "Amazon Marketplace");
    }

    #[test]
    fn test_full_pipeline_idempotent() {
        let r = rules();
        let (first, _) = normalise_payee("WOOLWORTHS 1234 STRATHFIELD", &r);
        let (second, _) = normalise_payee(&first, &r);
        assert_eq!(first, second, "Pipeline should be idempotent on its own output");
    }
}
