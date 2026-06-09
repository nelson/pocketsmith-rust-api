//! The single-string tester (rule-cli §3.4): the GUI inline tester /
//! CLI `rule test`. Compiles one candidate rule the way its stage does
//! at runtime and reports match / miss / syntax error.

use rusqlite::Connection;

use super::super::model::RuleData;
use super::super::Stage;

/// Outcome of the single-string tester.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestResult {
    /// The candidate matched; `canonical` is its output (empty for
    /// stages without one), `span` the matched byte range in `input`.
    Matches { canonical: String, span: Option<(usize, usize)> },
    Misses,
    SyntaxError(String),
}

/// Test a single candidate rule against one input string, mirroring how
/// the stage compiles its pattern at runtime.
pub fn test_one(_conn: &Connection, stage: Stage, candidate: &RuleData, input: &str) -> TestResult {
    let pattern = match candidate.pattern() {
        Some(p) => p,
        // Locations match on text, not a pattern.
        None => return test_location(candidate, input),
    };
    let re = match compile_for_stage(stage, pattern) {
        Ok(re) => re,
        Err(e) => return TestResult::SyntaxError(e.to_string()),
    };
    match re.find(input) {
        Some(m) => TestResult::Matches {
            canonical: candidate.canonical().unwrap_or("").to_string(),
            span: Some((m.start(), m.end())),
        },
        None => TestResult::Misses,
    }
}

fn test_location(candidate: &RuleData, input: &str) -> TestResult {
    let loc = candidate.canonical().unwrap_or("");
    let re = match regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(loc))) {
        Ok(re) => re,
        Err(e) => return TestResult::SyntaxError(e.to_string()),
    };
    match re.find(input) {
        Some(m) => TestResult::Matches { canonical: loc.to_string(), span: Some((m.start(), m.end())) },
        None => TestResult::Misses,
    }
}

/// Compile a candidate's pattern the way `stage` does at runtime: raw
/// regex for prefix/suffix/merchant/employer/banking_ops; escaped
/// word-boundary literal for expansion/person.
fn compile_for_stage(stage: Stage, pattern: &str) -> Result<regex::Regex, regex::Error> {
    match stage {
        Stage::Expansions => regex::Regex::new(&format!("(?i)\\b{}\\b", regex::escape(pattern))),
        Stage::Persons => {
            regex::Regex::new(&format!(r"(?i)\b{}(?:\b|\s|$)", regex::escape(pattern)))
        }
        _ => regex::Regex::new(pattern),
    }
}
