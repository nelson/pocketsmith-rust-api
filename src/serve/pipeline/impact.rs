//! HTML rendering of the evaluate impact buckets + single-string tester
//! (editable-rules-ui §3.5). The *computation* is the shared library
//! (`rules::impact::{compute_buckets, test_one}`); this module only
//! projects the resulting [`Buckets`] / [`TestResult`] into the markup
//! the mockups describe. First-match stages show four categories; loop
//! stages collapse to two.

use maud::{html, Markup};

use pocketsmith_sync::rules::impact::{BucketCount, Buckets, TestResult, SAMPLE_LIMIT};

use crate::serve::helpers::format_dollars_compact;

/// Everything the evaluate section needs.
pub struct Eval<'a> {
    /// The tester input value (carried across Evaluate re-posts).
    pub test_string: &'a str,
    /// Tester outcome; `None` when the tester box is empty.
    pub test_result: Option<&'a TestResult>,
    /// The bucketed dry-run result.
    pub buckets: &'a Buckets,
    /// Distinct payees the evaluation ran over (for the "against N payees").
    pub payee_total: i64,
    /// Endpoint the tester input re-posts to on change (re-runs evaluate).
    pub evaluate_url: &'a str,
}

/// Render the tester + impact tables block embedded inside the editor
/// card's evaluate mode.
pub fn render(e: &Eval) -> Markup {
    html! {
        div.eval-section {
            h3 { "Test against a string" }
            div.tester-input-row {
                label for="t-string" { "String" }
                input #t-string type="text" name="test_string" value=(e.test_string)
                    placeholder="paste a raw payee to test"
                    hx-post=(e.evaluate_url) hx-target="#editor-col" hx-swap="innerHTML"
                    hx-trigger="change, keyup changed delay:400ms" hx-include="#rule-form"
                    hx-indicator="#card-spin";
            }
            (render_test_result(e.test_string, e.test_result))
        }
        div.eval-section {
            h3 { "Impact across the database" }
            div.sub {
                "Compared to the saved rule, against "
                strong { (commas(e.payee_total)) }
                " distinct raw payees."
            }
            (render_buckets(e.buckets))
        }
    }
}

/// The tester result line: hit (→ canonical), miss, or syntax error.
fn render_test_result(input: &str, result: Option<&TestResult>) -> Markup {
    let Some(result) = result else {
        return html! {};
    };
    match result {
        TestResult::Matches { canonical, span } => {
            let matched = span
                .and_then(|(s, end)| input.get(s..end))
                .filter(|m| !m.is_empty());
            html! {
                div.tester-result {
                    span.tester-hit { "\u{2713} matches" }
                    @if !canonical.is_empty() {
                        span.tester-canon { " \u{2192} " strong { (canonical) } }
                    }
                    @if let Some(m) = matched {
                        span.tester-span { " (matched \u{201c}" (m) "\u{201d})" }
                    }
                }
            }
        }
        TestResult::Misses => html! {
            div.tester-result { span.tester-miss { "\u{2717} no match" } }
        },
        TestResult::SyntaxError(msg) => html! {
            div.tester-result { span.tester-miss { "syntax error: " (msg) } }
        },
    }
}

/// One outcome category, used for both the summary and detail tables.
#[derive(Clone, Copy)]
enum Outcome {
    Gain,
    Moved,
    Fall,
}

impl Outcome {
    fn glyph(self) -> &'static str {
        match self {
            Outcome::Gain => "+",
            Outcome::Moved => "\u{00b1}",
            Outcome::Fall => "\u{2212}",
        }
    }
    fn class(self) -> &'static str {
        match self {
            Outcome::Gain => "imp-gain",
            Outcome::Moved => "imp-move",
            Outcome::Fall => "imp-fall",
        }
    }
}

struct Section<'a> {
    outcome: Outcome,
    label: &'a str,
    bucket: &'a BucketCount,
}

/// Render the summary + detail tables (CLI `evaluate` layout). Public so
/// the delete-preview card can show the deletion impact without the
/// tester (which is meaningless for a delete).
pub fn render_buckets(buckets: &Buckets) -> Markup {
    let (sections, unchanged): (Vec<Section>, i64) = match buckets {
        Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => (
            vec![
                Section { outcome: Outcome::Gain, label: "newly matched", bucket: newly_matched },
                Section { outcome: Outcome::Moved, label: "moved", bucket: stolen },
                Section { outcome: Outcome::Fall, label: "new fallthrough", bucket: new_fallthrough },
            ],
            *unchanged_payees,
        ),
        Buckets::Loop { newly_affected, no_longer_affected, unchanged_payees } => (
            vec![
                Section { outcome: Outcome::Gain, label: "newly affected", bucket: newly_affected },
                Section { outcome: Outcome::Fall, label: "no longer affected", bucket: no_longer_affected },
            ],
            *unchanged_payees,
        ),
    };
    html! {
        (summary_table(&sections, unchanged))
        (detail_table(&sections))
    }
}

/// One-row-per-outcome summary: Outcome | Payees | Txns | Value.
fn summary_table(sections: &[Section], unchanged: i64) -> Markup {
    html! {
        table.impact-table.impact-summary-tbl {
            thead {
                tr { th.l { "Outcome" } th.r { "Payees" } th.r { "Txns" } th.r { "Value" } }
            }
            tbody {
                @for s in sections {
                    tr {
                        td { span.(s.outcome.class()) { (s.outcome.glyph()) " " (s.label) } }
                        td.r { (commas(s.bucket.payees)) }
                        td.r { (commas(s.bucket.txns)) }
                        td.r { (money(s.bucket.total_cents)) }
                    }
                }
                tr.row-unchanged {
                    td { span.imp-unchanged { "\u{00b7} unchanged" } }
                    td.r { (commas(unchanged)) }
                    td.r { "\u{2014}" }
                    td.r { "\u{2014}" }
                }
            }
        }
    }
}

/// Detail of changed payees: Payee | Txns | Value | Was | Now, with the
/// outcome glyph colouring each payee. The first [`SAMPLE_LIMIT`] rows of
/// each bucket show inline; the rest are hidden until a “show N more” row
/// is clicked, which reveals **everything** (no cap) via a `show-all`
/// class on the table.
fn detail_table(sections: &[Section]) -> Markup {
    let any = sections.iter().any(|s| !s.bucket.samples.is_empty());
    if !any {
        return html! { div.impact-none { "No payees change." } };
    }
    html! {
        table.impact-table.impact-detail {
            thead {
                tr { th.l { "Payee" } th.r { "Txns" } th.r { "Value" } th.l { "Was" } th.l { "Now" } }
            }
            tbody {
                @for s in sections {
                    @for (i, sample) in s.bucket.samples.iter().enumerate() {
                        tr.(if i >= SAMPLE_LIMIT { "impact-extra" } else { "" }) {
                            td.payee {
                                span.(s.outcome.class()) { (s.outcome.glyph()) }
                                " " (sample.original_payee)
                            }
                            td.r { (commas(sample.txn_count)) }
                            td.r { (money(sample.total_cents)) }
                            td { (opt_cell(&sample.was)) }
                            td { (opt_cell(&sample.now)) }
                        }
                    }
                    @if s.bucket.samples.len() > SAMPLE_LIMIT {
                        tr.impact-more {
                            td colspan="5" onclick="this.closest('table').classList.add('show-all')" {
                                "show " (s.bucket.samples.len() - SAMPLE_LIMIT) " more " (s.label)
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A was/now cell value, or a dim em-dash for None.
fn opt_cell(v: &Option<String>) -> Markup {
    match v {
        Some(v) => html! { (v) },
        None => html! { span.cell-null { "\u{2014}" } },
    }
}

fn money(cents: i64) -> String {
    format_dollars_compact(cents)
}

/// Group an integer with thousands separators (e.g. 1234 → "1,234").
fn commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let mut grouped: String = out.chars().rev().collect();
    if n < 0 {
        grouped.insert(0, '-');
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::rules::impact::BucketSample;

    fn sample(payee: &str, txns: i64) -> BucketSample {
        BucketSample {
            original_payee: payee.into(),
            txn_count: txns,
            total_cents: txns * 1000,
            account: Some("Amex".into()),
            was: None,
            now: Some("Amazon".into()),
        }
    }

    fn first_match() -> Buckets {
        let mut nm = BucketCount::default();
        for i in 0..8 {
            nm.samples.push(sample(&format!("PAYEE {i}"), 3));
        }
        nm.payees = 8;
        nm.txns = 24;
        Buckets::FirstMatch {
            newly_matched: nm,
            stolen: BucketCount::default(),
            new_fallthrough: BucketCount::default(),
            unchanged_payees: 31,
        }
    }

    #[test]
    fn renders_four_first_match_categories() {
        let b = first_match();
        let e = Eval {
            test_string: "",
            test_result: None,
            buckets: &b,
            payee_total: 423,
            evaluate_url: "/pipeline/stage/merchants/rule/7/evaluate",
        };
        let h = render(&e).into_string();
        assert!(h.contains("impact-summary-tbl"), "summary table: {h}");
        assert!(h.contains("impact-detail"), "detail table: {h}");
        assert!(h.contains("newly matched"), "{h}");
        assert!(h.contains("new fallthrough"), "{h}");
        assert!(h.contains("moved"), "{h}");
        assert!(h.contains("unchanged"), "{h}");
        assert!(h.contains("423"), "payee total: {h}");
    }

    #[test]
    fn overflow_samples_show_expandable_rows() {
        // 8 samples, SAMPLE_LIMIT=6 → 2 hidden `impact-extra` rows + a
        // clickable "show 2 more" toggle that reveals everything.
        let b = first_match();
        let e = Eval {
            test_string: "",
            test_result: None,
            buckets: &b,
            payee_total: 8,
            evaluate_url: "x",
        };
        let h = render(&e).into_string();
        assert!(h.contains("show 2 more"), "{h}");
        assert!(h.contains("impact-more"), "{h}");
        assert!(h.contains("impact-extra"), "hidden rows present: {h}");
        assert!(h.contains("show-all"), "toggle reveals everything: {h}");
    }

    #[test]
    fn loop_stage_has_two_categories() {
        let mut na = BucketCount::default();
        na.samples.push(sample("POS 0241 WOOLWORTHS", 4));
        na.payees = 1;
        na.txns = 4;
        let b = Buckets::Loop {
            newly_affected: na,
            no_longer_affected: BucketCount::default(),
            unchanged_payees: 5,
        };
        let e = Eval { test_string: "", test_result: None, buckets: &b, payee_total: 6, evaluate_url: "x" };
        let h = render(&e).into_string();
        assert!(h.contains("newly affected"), "{h}");
        assert!(h.contains("no longer affected"), "{h}");
        assert!(!h.contains("stolen"), "loop stages have no stolen bucket: {h}");
        assert!(h.contains("POS 0241 WOOLWORTHS"), "{h}");
    }

    #[test]
    fn tester_hit_miss_and_syntax() {
        let b = Buckets::Loop {
            newly_affected: BucketCount::default(),
            no_longer_affected: BucketCount::default(),
            unchanged_payees: 0,
        };
        let hit = TestResult::Matches { canonical: "Amazon".into(), span: Some((0, 6)) };
        let h = render(&Eval {
            test_string: "AMAZON AU",
            test_result: Some(&hit),
            buckets: &b,
            payee_total: 0,
            evaluate_url: "x",
        })
        .into_string();
        assert!(h.contains("\u{2713} matches"), "{h}");
        assert!(h.contains("Amazon"), "{h}");

        let miss = TestResult::Misses;
        let h = render(&Eval {
            test_string: "OPAL",
            test_result: Some(&miss),
            buckets: &b,
            payee_total: 0,
            evaluate_url: "x",
        })
        .into_string();
        assert!(h.contains("\u{2717} no match"), "{h}");

        let err = TestResult::SyntaxError("unbalanced".into());
        let h = render(&Eval {
            test_string: "x",
            test_result: Some(&err),
            buckets: &b,
            payee_total: 0,
            evaluate_url: "x",
        })
        .into_string();
        assert!(h.contains("syntax error: unbalanced"), "{h}");
    }
}
