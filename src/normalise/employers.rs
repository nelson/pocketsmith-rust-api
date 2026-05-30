#[cfg(test)]
use std::sync::OnceLock;

use regex::Regex;

use super::{NormalisationResult, PayeeClass};

#[cfg(test)]
struct Employer {
    canonical: &'static str,
    patterns: &'static [&'static str],
}

pub(crate) struct CompiledEmployer {
    regex: Regex,
    canonical: String,
}

/// First-match-wins over the compiled set, but only if nothing has
/// classified the payee yet (persons run first).
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledEmployer]) {
    if result.class().is_some() {
        return;
    }
    for ce in compiled {
        if ce.regex.is_match(&result.normalised) {
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

/// Load + compile employer rules in declaration order (`id`).
pub(crate) fn load_compiled(conn: &rusqlite::Connection) -> anyhow::Result<Vec<CompiledEmployer>> {
    let mut stmt =
        conn.prepare("SELECT canonical, pattern FROM rule_employers ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (canonical, pattern) = r?;
        let regex = Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("invalid employer pattern {pattern:?}: {e}"))?;
        out.push(CompiledEmployer { regex, canonical });
    }
    Ok(out)
}

/// Const-backed employer match: fidelity oracle.
#[cfg(test)]
pub(crate) fn apply(result: &mut NormalisationResult) {
    run_match(result, compiled_employers());
}
#[cfg(test)]
const KNOWN_EMPLOYERS: &[Employer] = &[
    Employer {
        canonical: "AFES",
        patterns: &[
            r"(?i)(?:Salary from|From) AFES",
        ],
    },
    Employer {
        canonical: "Apple",
        patterns: &[
            r"(?i)(?:PAY/SALARY FROM|Salary from|TRANSFER FROM|From) APPLE (?:COMPUTERS|PTY LTD|COMPUTER AUSTRALIA)",
            r"(?i)Employer Contribution From Apple",
        ],
    },
    Employer {
        canonical: "Freelancer",
        patterns: &[
            r"(?i)(?:Salary.*Freelancer|Employer Contribution From Freelancer)",
        ],
    },
    Employer {
        canonical: "Ghost Locomotion",
        patterns: &[
            r"(?i)(?:Salary.*GHOST LOCOMOTION|Employer Contribution From Ghost Locomotion|Ghost Locomotion.*(?:Receipt|Salary))",
        ],
    },
];

#[cfg(test)]
fn compiled_employers() -> &'static [CompiledEmployer] {
    static COMPILED: OnceLock<Vec<CompiledEmployer>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        KNOWN_EMPLOYERS
            .iter()
            .flat_map(|e| {
                e.patterns.iter().map(move |&pat| CompiledEmployer {
                    regex: Regex::new(pat).expect("invalid employer pattern"),
                    canonical: e.canonical.to_string(),
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DB-backed employer match must reproduce the const oracle.
    #[test]
    fn db_apply_matches_const_oracle() {
        let p = crate::normalise::OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        for inp in [
            "PAY/SALARY FROM APPLE COMPUTERS SALARY",
            "Salary from AFES - TAM S AFES",
            "APPLE STORE R523 R523 BROADWAY",
            "Employer Contribution From Apple",
            "Salary Freelancer",
        ] {
            let mut a = NormalisationResult::new(inp);
            apply(&mut a);
            let mut b = NormalisationResult::new(inp);
            apply_with_db(&mut b, &ctx);
            assert_eq!(a.features.entity_name, b.features.entity_name, "entity differs for {inp:?}");
            assert_eq!(a.class(), b.class(), "class differs for {inp:?}");
        }
    }

    #[test]
    fn test_employer_apple_salary() {
        let mut r = NormalisationResult::new("PAY/SALARY FROM APPLE COMPUTERS SALARY");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("Apple"));
        assert_eq!(r.class(), Some(&PayeeClass::Employer));
    }

    #[test]
    fn test_employer_afes_salary() {
        let mut r = NormalisationResult::new("Salary from AFES - TAM S AFES");
        apply(&mut r);
        assert_eq!(r.features.entity_name.as_deref(), Some("AFES"));
        assert_eq!(r.class(), Some(&PayeeClass::Employer));
    }

    #[test]
    fn test_not_employer_apple_store() {
        let mut r = NormalisationResult::new("APPLE STORE R523 R523 BROADWAY");
        apply(&mut r);
        assert!(r.features.entity_name.is_none());
        assert!(r.class().is_none());
    }
}
