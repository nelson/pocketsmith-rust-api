use regex::Regex;

use super::{NormalisationResult, PayeeClass};

pub(crate) struct CompiledMerchant {
    regex: Regex,
    canonical: String,
    pattern: String,
}

/// First-match-wins over the compiled set, only if unclassified.
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledMerchant]) {
    if result.class().is_some() {
        return;
    }
    for cm in compiled {
        if let Some(m) = cm.regex.find(&result.normalised) {
            result.record_match(cm.pattern.clone(), Some((m.start(), m.end())));
            result.features.entity_name = Some(cm.canonical.clone());
            result.set_class(PayeeClass::Merchant);
            return;
        }
    }
}

/// DB-backed merchant match.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &super::PipelineCtx) {
    match ctx.cache.merchants(ctx.conn) {
        Ok(compiled) => run_match(result, &compiled),
        Err(e) => eprintln!("merchants: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load + compile merchant rules in declaration order (`id`), so the
/// "more specific pattern must appear first" invariant is preserved.
/// Rule rows come from the typed [`crud::load_for_compile`] read — the
/// column shape lives once in `rules::model` (rule-cli §3.1).
pub(crate) fn load_compiled(conn: &rusqlite::Connection) -> anyhow::Result<Vec<CompiledMerchant>> {
    use crate::rules::{crud, model::RuleData, Stage};
    let mut out = Vec::new();
    for data in crud::load_for_compile(conn, Stage::Merchants)? {
        if let RuleData::Merchant { canonical, pattern, .. } = data {
            let regex = Regex::new(&pattern)
                .map_err(|e| anyhow::anyhow!("invalid merchant pattern {pattern:?}: {e}"))?;
            out.push(CompiledMerchant { regex, canonical, pattern });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::OwnedPipeline;

    /// Run the DB-backed merchant stage against the seeded in-memory
    /// pipeline (rules loaded from `rules/merchants.sql`). Seeded once
    /// per test thread.
    fn run(input: &str) -> NormalisationResult {
        thread_local! {
            static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
        }
        PIPELINE.with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new(input);
            apply_with_db(&mut r, &ctx);
            r
        })
    }

    fn assert_merchant(input: &str, expected: &str) {
        assert_eq!(run(input).features.entity_name.as_deref(), Some(expected));
    }

    #[test]
    fn test_skip_if_classified() {
        thread_local! {
            static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
        }
        PIPELINE.with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new("WOOLWORTHS");
            r.set_class(PayeeClass::Person);
            apply_with_db(&mut r, &ctx);
            assert!(r.features.entity_name.is_none());
        });
    }

    // Tricky regex intent worth guarding against accidental seed edits:
    // no-space run-on, apostrophe-optional, alternations, and ordering
    // (more specific pattern must win over the generic one).

    #[test]
    fn test_transport_nsw_no_spaces() {
        assert_merchant("TRANSPORTFORNSWTRAVEL SYDNEY", "Transport for NSW");
    }

    #[test]
    fn test_diggy_doos_no_apostrophe() {
        assert_merchant("DIGGY DOOS COFFEE Sydney", "Diggy Doo's Coffee");
    }

    #[test]
    fn test_mamaks_mlc() {
        assert_merchant("MAMAKSMLC XX2906 SYDNEY", "Mamak");
    }

    #[test]
    fn test_uber_star_eats_orders_before_bare_uber() {
        assert_merchant("UBER *EATS Sydney AU AUS", "Uber Eats");
    }

    #[test]
    fn test_amazon_prime_orders_before_bare_amazon() {
        assert_merchant("AMAZON PRIME AU", "Amazon Prime");
    }

    #[test]
    fn test_regiment_speciality_truncated() {
        assert_merchant("REGIMENT SPECIALITY CAF Sydney", "Regiment Coffee");
    }
}
