use super::pipeline::CategoryChange;
use super::{CategoriseResult, CategoriseSource};

pub struct Report {
    pub total_payees: usize,
    pub by_rule: usize,
    pub by_cache: usize,
    pub by_google: usize,
    pub by_llm: usize,
    pub uncategorised: usize,
    pub total_changes: usize,
    pub total_txns_affected: usize,
}

pub fn build_report(results: &[CategoriseResult], changes: &[CategoryChange]) -> Report {
    let total = results.len();

    let mut by_rule = 0usize;
    let mut by_cache = 0usize;
    let mut by_google = 0usize;
    let mut by_llm = 0usize;
    let mut uncategorised = 0usize;

    for r in results {
        match (&r.source, &r.category) {
            (CategoriseSource::Rule, Some(_)) => by_rule += 1,
            (CategoriseSource::Cache, Some(_)) => by_cache += 1,
            (CategoriseSource::GooglePlaces, Some(_)) => by_google += 1,
            (CategoriseSource::Llm, Some(_)) => by_llm += 1,
            _ => uncategorised += 1,
        }
    }

    let total_txns_affected: usize = changes.iter().map(|c| c.txn_count).sum();

    Report {
        total_payees: total,
        by_rule,
        by_cache,
        by_google,
        by_llm,
        uncategorised,
        total_changes: changes.len(),
        total_txns_affected,
    }
}

pub fn print_report(report: &Report) {
    let t = report.total_payees;
    let pct = |n: usize| if t > 0 { n as f64 / t as f64 * 100.0 } else { 0.0 };

    println!("\n=== Categorisation Report ===");
    println!("Total unique payees: {}", t);
    println!(
        "  By rules:         {:>4} ({:>5.1}%)",
        report.by_rule, pct(report.by_rule)
    );
    println!(
        "  By cache:         {:>4} ({:>5.1}%)",
        report.by_cache, pct(report.by_cache)
    );
    println!(
        "  By Google Places: {:>4} ({:>5.1}%)",
        report.by_google, pct(report.by_google)
    );
    println!(
        "  By LLM:           {:>4} ({:>5.1}%)",
        report.by_llm, pct(report.by_llm)
    );
    println!(
        "  Uncategorised:    {:>4} ({:>5.1}%)",
        report.uncategorised, pct(report.uncategorised)
    );
}

pub fn print_changes(changes: &[CategoryChange], limit: usize) {
    if changes.is_empty() {
        println!("\nNo category changes needed.");
        return;
    }

    println!("\n=== Category changes (by frequency) ===");
    for (i, c) in changes.iter().take(limit).enumerate() {
        let old = c
            .old_category
            .as_deref()
            .unwrap_or("Uncategorised");
        println!(
            "  {} ({} txns): {} → {}  [{}]",
            c.normalised_payee, c.txn_count, old, c.new_category, c.reason
        );
        if i >= limit - 1 && changes.len() > limit {
            println!("  ... and {} more", changes.len() - limit);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(source: CategoriseSource, category: Option<&str>) -> CategoriseResult {
        CategoriseResult {
            normalised_payee: "Test".into(),
            category: category.map(|s| s.into()),
            source,
            reason: "test".into(),
            transaction_count: 1,
        }
    }

    #[test]
    fn test_report_counts() {
        let results = vec![
            make_result(CategoriseSource::Rule, Some("_Income")),
            make_result(CategoriseSource::Rule, Some("_Transfer")),
            make_result(CategoriseSource::Cache, Some("_Groceries")),
            make_result(CategoriseSource::GooglePlaces, Some("_Dining")),
            make_result(CategoriseSource::Llm, Some("_Bills")),
            make_result(CategoriseSource::Unknown, None),
        ];

        let report = build_report(&results, &[]);
        assert_eq!(report.total_payees, 6);
        assert_eq!(report.by_rule, 2);
        assert_eq!(report.by_cache, 1);
        assert_eq!(report.by_google, 1);
        assert_eq!(report.by_llm, 1);
        assert_eq!(report.uncategorised, 1);
    }

    #[test]
    fn test_report_empty() {
        let report = build_report(&[], &[]);
        assert_eq!(report.total_payees, 0);
        assert_eq!(report.uncategorised, 0);
    }
}
