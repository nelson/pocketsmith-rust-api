use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;

use super::{NormalisationResult, PayeeClass, PipelineCtx};

pub(crate) struct CompiledPerson {
    regex: Regex,
    canonical: String,
    /// The authored pattern (raw literal), surfaced in the pipeline trace.
    pattern: String,
}

/// Compile one person pattern the way the const path does: a
/// case-insensitive literal anchored on a leading word boundary and a
/// trailing boundary/space/end.
fn compile_person(pattern: &str) -> Regex {
    Regex::new(&format!(r"(?i)\b{}(?:\b|\s|$)", regex::escape(pattern)))
        .expect("invalid person pattern")
}

/// First-match-wins over the compiled set: set entity_name + class.
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledPerson]) {
    for cp in compiled {
        if let Some(m) = cp.regex.find(&result.normalised) {
            result.record_match(cp.pattern.clone(), Some((m.start(), m.end())));
            result.features.entity_name = Some(cp.canonical.clone());
            result.set_class(PayeeClass::Person);
            return;
        }
    }
}

/// DB-backed person match.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &PipelineCtx) {
    match ctx.cache.persons(ctx.conn) {
        Ok(compiled) => run_match(result, &compiled),
        Err(e) => eprintln!("persons: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load + compile person rules. Ordered by `id` (= declaration /
/// insertion order) so the order-sensitive generic fallbacks
/// (`MR`/`MISS`/`MRS`, single-token names) stay last under
/// first-match-wins. See progress note Decision #2.
pub(crate) fn load_compiled(conn: &Connection) -> Result<Vec<CompiledPerson>> {
    let mut stmt =
        conn.prepare("SELECT canonical, pattern FROM rule_persons ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (canonical, pattern) = r?;
        out.push(CompiledPerson {
            regex: compile_person(&pattern),
            canonical,
            pattern,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::OwnedPipeline;

    /// Run the DB-backed person stage against the seeded in-memory
    /// pipeline (rules from `src/rules/persons.sql`).
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
    fn test_person_johnny_tam() {
        let r = run("JOHNNY TAM");
        assert_eq!(r.features.entity_name.as_deref(), Some("Johnny Tam"));
        assert_eq!(r.class(), Some(&PayeeClass::Person));
    }

    #[test]
    fn test_person_with_prefix() {
        let r = run("TRANSFER FROM NELSON TAM");
        assert_eq!(r.features.entity_name.as_deref(), Some("Nelson Tam"));
        assert_eq!(r.class(), Some(&PayeeClass::Person));
    }

    #[test]
    fn test_person_no_match() {
        let r = run("WOOLWORTHS STRATHFIELD");
        assert!(r.features.entity_name.is_none());
        assert!(r.class().is_none());
    }
}
