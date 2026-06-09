use regex::Regex;

use super::{NormalisationResult, PayeeClass};

pub(crate) struct CompiledEmployer {
    regex: Regex,
    canonical: String,
    pattern: String,
}

/// First-match-wins over the compiled set, but only if nothing has
/// classified the payee yet (persons run first).
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledEmployer]) {
    if result.class().is_some() {
        return;
    }
    for ce in compiled {
        if let Some(m) = ce.regex.find(&result.normalised) {
            result.record_match(ce.pattern.clone(), Some((m.start(), m.end())));
            result.features.entity_name = Some(ce.canonical.clone());
            result.set_class(PayeeClass::Employer);
            return;
        }
    }
}

/// DB-backed employer match.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &super::PipelineCtx) {
    match ctx.cache.employers(ctx.conn) {
        Ok(compiled) => run_match(result, &compiled),
        Err(e) => eprintln!("employers: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load + compile employer rules in declaration order (`id`). Rows come
/// from the typed [`crud::load_for_compile`] read (rule-cli §3.1).
pub(crate) fn load_compiled(conn: &rusqlite::Connection) -> anyhow::Result<Vec<CompiledEmployer>> {
    use crate::rules::{crud, model::RuleData, Stage};
    let mut out = Vec::new();
    for data in crud::load_for_compile(conn, Stage::Employers)? {
        if let RuleData::Employer { canonical, pattern, .. } = data {
            let regex = Regex::new(&pattern)
                .map_err(|e| anyhow::anyhow!("invalid employer pattern {pattern:?}: {e}"))?;
            out.push(CompiledEmployer { regex, canonical, pattern });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::OwnedPipeline;

    /// Run the DB-backed employer stage against the seeded in-memory
    /// pipeline (rules from `rules/employers.sql`).
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

    #[test]
    fn test_employer_apple_salary() {
        let r = run("PAY/SALARY FROM APPLE COMPUTERS SALARY");
        assert_eq!(r.features.entity_name.as_deref(), Some("Apple"));
        assert_eq!(r.class(), Some(&PayeeClass::Employer));
    }

    #[test]
    fn test_employer_afes_salary() {
        let r = run("Salary from AFES - TAM S AFES");
        assert_eq!(r.features.entity_name.as_deref(), Some("AFES"));
        assert_eq!(r.class(), Some(&PayeeClass::Employer));
    }

    #[test]
    fn test_not_employer_apple_store() {
        let r = run("APPLE STORE R523 R523 BROADWAY");
        assert!(r.features.entity_name.is_none());
        assert!(r.class().is_none());
    }
}
