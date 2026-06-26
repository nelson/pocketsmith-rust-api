//! Shared pipeline-trace renderer (editable rules UI — PR 1).
//!
//! Both the Transactions detail panel and the Normalise tab render the
//! same `NormalisationResult::trace`, so the markup lives here once.
//!
//! Every stage entry renders a **uniform two-line structure**:
//!   * **Line 1** — the string transform `{before} → {after}` for stages
//!     that change the string (prefix/suffix/expand/…); or, for matcher
//!     stages that classify without changing the string (persons /
//!     employers / merchants / locations / banking-op classify), the
//!     match line `{pattern} ~= {string}` with the matched substring
//!     highlighted green.
//!   * **Line 2** — the extracted features / class (`+entity_name (…)`,
//!     `class = merchant`, …).

use std::collections::HashMap;

use maud::{html, Markup};

use pocketsmith_sync::normalise::{MatchInfo, NormalisationResult, TraceEntry};

/// Render the per-stage transformation trace for a normalisation result.
/// One row per pipeline stage that changed the string, attached a
/// feature, or matched a rule.
pub fn render_pipeline_trace(p: &NormalisationResult) -> Markup {
    if p.trace.is_empty() {
        return html! {
            div.norm-trace {
                h3 { "Pipeline trace" }
                div.norm-trace-empty { "(no rules matched \u{2014} normalised string equals the original)" }
            }
        };
    }
    html! {
        div.norm-trace {
            h3 { "Pipeline trace" }
            div.norm-trace-list {
                @for entry in &p.trace {
                    (render_trace_entry(entry))
                }
            }
        }
    }
}

fn render_trace_entry(entry: &TraceEntry) -> Markup {
    let changed_string = entry.before != entry.after;
    let values: HashMap<&str, &str> = entry
        .feature_values
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    html! {
        div.norm-trace-row {
            span.norm-trace-stage { (entry.stage) }
            div.norm-trace-body {
                // Line 1: string diff, else a rule-match line.
                @if changed_string {
                    div.norm-trace-diff {
                        span.norm-trace-before { (entry.before) }
                        span.norm-trace-arrow { " \u{2192} " }
                        span.norm-trace-after { (entry.after) }
                    }
                } @else if let Some(mi) = &entry.match_info {
                    (render_match_line(mi))
                }
                // Line 2: extracted features / class.
                @if !entry.features_added.is_empty() || entry.class_set.is_some() {
                    div.norm-trace-extracted {
                        @if let Some(c) = &entry.class_set {
                            span.norm-trace-class { "class = " (format!("{:?}", c).to_lowercase()) }
                        }
                        @for feat in &entry.features_added {
                            @if let Some(v) = values.get(feat) {
                                span.norm-trace-feat {
                                    "+" (feat) " "
                                    span.norm-trace-feat-val { "(" (v) ")" }
                                }
                            } @else {
                                span.norm-trace-feat { "+" (feat) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `{pattern} ~= {string}` with the matched substring highlighted green.
/// Falls back to the whole string un-highlighted when the span is absent
/// or doesn't land on char boundaries (defensive — spans from the
/// pipeline always do).
fn render_match_line(mi: &MatchInfo) -> Markup {
    let valid_span = mi.span.filter(|&(s, e)| {
        s <= e
            && e <= mi.haystack.len()
            && mi.haystack.is_char_boundary(s)
            && mi.haystack.is_char_boundary(e)
    });
    html! {
        div.norm-trace-match {
            span.norm-trace-pattern { (mi.pattern) }
            span.norm-trace-tilde { " ~= " }
            @match valid_span {
                Some((s, e)) => {
                    span.norm-trace-hay { (mi.haystack[..s]) }
                    span.norm-trace-hay-hit { (mi.haystack[s..e]) }
                    span.norm-trace-hay { (mi.haystack[e..]) }
                }
                None => span.norm-trace-hay { (mi.haystack) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::normalise::PayeeClass;

    fn diff_entry() -> TraceEntry {
        TraceEntry {
            stage: "prefix",
            before: "PURCHASE SHOP".into(),
            after: "SHOP".into(),
            features_added: Vec::new(),
            feature_values: Vec::new(),
            class_set: None,
            match_info: None,
            fired: Vec::new(),
        }
    }

    fn match_entry() -> TraceEntry {
        TraceEntry {
            stage: "merchants",
            before: "PURCHASE ZEBRA CAFE SYDNEY".into(),
            after: "PURCHASE ZEBRA CAFE SYDNEY".into(),
            features_added: vec!["entity_name"],
            feature_values: vec![("entity_name", "Zebra Cafe".into())],
            class_set: Some(PayeeClass::Merchant),
            match_info: Some(MatchInfo {
                pattern: "(?i)ZEBRA CAFE".into(),
                haystack: "PURCHASE ZEBRA CAFE SYDNEY".into(),
                span: Some((9, 19)),
            }),
            fired: Vec::new(),
        }
    }

    #[test]
    fn modifying_stage_renders_diff_not_match_line() {
        let html = render_trace_entry(&diff_entry()).into_string();
        assert!(html.contains("norm-trace-diff"), "{html}");
        assert!(html.contains("\u{2192}"), "has the arrow: {html}");
        assert!(!html.contains("norm-trace-match"), "no match line: {html}");
        assert!(!html.contains(" ~= "), "no tilde: {html}");
    }

    #[test]
    fn matcher_stage_renders_pattern_tilde_and_green_hit() {
        let html = render_trace_entry(&match_entry()).into_string();
        // Line 1: pattern ~= string, with the matched substring in the
        // green hit span.
        assert!(html.contains("norm-trace-match"), "{html}");
        assert!(html.contains("norm-trace-pattern"), "{html}");
        assert!(html.contains("(?i)ZEBRA CAFE"), "shows the pattern: {html}");
        assert!(html.contains(" ~= "), "{html}");
        assert!(
            html.contains("<span class=\"norm-trace-hay-hit\">ZEBRA CAFE</span>"),
            "matched substring is the green hit: {html}"
        );
        // No diff arrow when the string didn't change.
        assert!(!html.contains("norm-trace-diff"), "{html}");
        // Line 2: features still render.
        assert!(html.contains("class = merchant"), "{html}");
        assert!(html.contains("+entity_name"), "{html}");
    }

    #[test]
    fn match_line_without_span_renders_whole_haystack() {
        let mut e = match_entry();
        e.match_info = Some(MatchInfo {
            pattern: "LOC".into(),
            haystack: "SOME PLACE".into(),
            span: None,
        });
        let html = render_trace_entry(&e).into_string();
        assert!(html.contains("norm-trace-match"), "{html}");
        assert!(!html.contains("norm-trace-hay-hit"), "no highlight without span: {html}");
        assert!(html.contains("SOME PLACE"), "{html}");
    }

    #[test]
    fn empty_trace_shows_placeholder() {
        let p = NormalisationResult::new("UNCHANGED");
        let html = render_pipeline_trace(&p).into_string();
        assert!(html.contains("norm-trace-empty"), "{html}");
    }
}
