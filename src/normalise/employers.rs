use std::sync::OnceLock;

use regex::Regex;

use super::{NormalisationResult, PayeeClass};

struct Employer {
    canonical: &'static str,
    patterns: &'static [&'static str],
}

struct CompiledEmployer {
    regex: Regex,
    canonical: &'static str,
}

pub fn apply(result: &mut NormalisationResult) {
    if result.class().is_some() {
        return;
    }
    for ce in compiled_employers() {
        if ce.regex.is_match(&result.normalised) {
            result.features.entity_name = Some(ce.canonical.to_string());
            result.set_class(PayeeClass::Employer);
            return;
        }
    }
}

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

fn compiled_employers() -> &'static [CompiledEmployer] {
    static COMPILED: OnceLock<Vec<CompiledEmployer>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        KNOWN_EMPLOYERS
            .iter()
            .flat_map(|e| {
                e.patterns.iter().map(move |&pat| CompiledEmployer {
                    regex: Regex::new(pat).expect("invalid employer pattern"),
                    canonical: e.canonical,
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
