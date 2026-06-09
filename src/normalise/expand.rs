use regex::Regex;

use anyhow::Result;
use rusqlite::Connection;

use super::{NormalisationResult, PipelineCtx};

pub(crate) struct CompiledExpansion {
    regex: Regex,
    canonical: String,
}

/// One run of the expand loop over a compiled rule set. Driven by the
/// DB-backed [`apply_with_db`].
fn run_loop(result: &mut NormalisationResult, compiled: &[CompiledExpansion]) {
    loop {
        let mut matched = false;
        for exp in compiled {
            if let Some(m) = exp.regex.find(&result.normalised) {
                result.normalised = format!(
                    "{}{}{}",
                    &result.normalised[..m.start()],
                    exp.canonical,
                    &result.normalised[m.end()..]
                );
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
}

/// Expand truncated words / country codes using the DB-backed rule set.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &PipelineCtx) {
    match ctx.cache.expansions(ctx.conn) {
        Ok(compiled) => run_loop(result, &compiled),
        Err(e) => eprintln!("expand: rule load failed, stage skipped: {e:#}"),
    }
}

/// Compile a single expansion pattern the same way the const path does:
/// case-insensitive, anchored on word boundaries.
fn compile_expansion(pattern: &str) -> Result<Regex> {
    Regex::new(&format!("(?i)\\b{}\\b", regex::escape(pattern)))
        .map_err(|e| anyhow::anyhow!("invalid expansion pattern {pattern:?}: {e}"))
}

/// Load + compile the expansion rules from the typed
/// [`crud::load_for_compile`] read in apply order (rule-cli §3.1).
pub(crate) fn load_compiled(conn: &Connection) -> Result<Vec<CompiledExpansion>> {
    use crate::rules::{crud, model::RuleData, Stage};
    let mut out = Vec::new();
    for data in crud::load_for_compile(conn, Stage::Expansions)? {
        if let RuleData::Expansion { pattern, canonical, .. } = data {
            out.push(CompiledExpansion { regex: compile_expansion(&pattern)?, canonical });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::{NormalisationResult, OwnedPipeline};

    /// Run the DB-backed expand stage against the seeded in-memory pipeline
    /// (rules from `rules/expansions.sql`).
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

    // Kept guards exercise run-on tokens, multi-word expansions, and a
    // mid-string expansion that keeps a trailing reference intact.

    #[test]
    fn test_expand_nswau() {
        assert_eq!(run("MERCHANT NSWAU").normalised, "MERCHANT NSW AU");
    }

    #[test]
    fn test_expand_mcare_benefits_keeps_trailing_digits() {
        assert_eq!(run("MCARE BENEFITS 024037941").normalised, "MEDICARE BENEFITS 024037941");
    }

    #[test]
    fn test_expand_pline_ph() {
        assert_eq!(run("PLINE PH STRATHFIELD").normalised, "PRICELINE PHARMACY STRATHFIELD");
    }

    #[test]
    fn test_expand_childassistpymt() {
        assert_eq!(run("CHILDASSISTPYMT").normalised, "CHILD ASSISTANCE PAYMENT");
    }

    #[test]
    fn test_expand_amznprimeau() {
        assert_eq!(run("AMZNPRIMEAU MEMBERSHIP").normalised, "AMAZON PRIME AU MEMBERSHIP");
    }
}
