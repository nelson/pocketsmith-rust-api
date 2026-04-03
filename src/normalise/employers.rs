struct EmployerPattern {
    pattern: &'static str,
    canonical: &'static str,
}

const KNOWN_EMPLOYERS: &[EmployerPattern] = &[
    EmployerPattern { pattern: "APPLE PTY LTD", canonical: "Apple" },
    EmployerPattern { pattern: "APPLE COMPUTERS", canonical: "Apple" },
    EmployerPattern { pattern: "APPLE COMPUTER AUSTRALIA PTY LTD", canonical: "Apple" },
    EmployerPattern { pattern: "AFES", canonical: "AFES" },
    EmployerPattern { pattern: "AUSTRALIAN FELLO", canonical: "AFES" },
];

/// Extract an employer from the original payee description.
/// Looks for salary-related patterns containing known employer names.
pub fn extract_employer(original: &str) -> Option<String> {
    let upper = original.to_uppercase();
    for emp in KNOWN_EMPLOYERS {
        if upper.contains(emp.pattern) {
            return Some(emp.canonical.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_employer_apple() {
        let result = extract_employer("Salary from Apple Pty Ltd");
        assert_eq!(result, Some("Apple".to_string()));
    }
}
