//! Human (text) output: tables, colour, the evaluate buckets + detail
//! grid. The `--json` branches delegate their payloads to `crate::json`.

use serde_json::to_string_pretty;

use pocketsmith_sync::rules::impact::{BucketCount, Buckets, TestResult, SAMPLE_LIMIT};
use pocketsmith_sync::rules::model::{Mutation, MoveTarget, Rule, RuleData};
use pocketsmith_sync::rules::{crud, CommitResult, Stage};

use crate::args::Flags;
use crate::colours::{highlight_regex, Style};
use crate::helpers::{commas, money, sanitize};
use crate::table::{render_table, Align, Cell};
use crate::{json, AppError};

// --- list ------------------------------------------------------------------

pub(crate) fn list(flags: &Flags, stage: Stage, rules: &[Rule]) {
    if flags.json {
        println!("{}", to_string_pretty(&json::list(stage, rules)).unwrap());
        return;
    }
    let style = Style::new(false);
    let ordered = crud::is_movable(stage);
    let header = if ordered {
        format!("STAGE {} — loop (apply order; reorder with `rule move`)", stage.name())
    } else {
        format!("STAGE {} — first-match-wins (auto-ordered)", stage.name())
    };
    println!("{}", style.bold(&header));
    println!();
    let table = if ordered {
        let cols = [("#", Align::Right), ("id", Align::Right), ("pattern", Align::Left)];
        let rows: Vec<Vec<Cell>> = rules
            .iter()
            .enumerate()
            .map(|(pos, r)| {
                let pat = r.data.pattern().unwrap_or("—");
                vec![
                    Cell::text(pos.to_string()),
                    Cell::text(r.id.to_string()),
                    Cell::coloured(pat, highlight_regex(&style, pat)),
                ]
            })
            .collect();
        render_table(&style, &cols, &rows)
    } else {
        let cols = [("id", Align::Right), ("canonical", Align::Left), ("pattern", Align::Left)];
        let rows: Vec<Vec<Cell>> = rules
            .iter()
            .map(|r| {
                let pat = r.data.pattern().unwrap_or("—");
                vec![
                    Cell::text(r.id.to_string()),
                    Cell::text(r.data.canonical().unwrap_or("—")),
                    Cell::coloured(pat, highlight_regex(&style, pat)),
                ]
            })
            .collect();
        render_table(&style, &cols, &rows)
    };
    print!("{table}");
    println!();
    println!(
        "{} rules.  `rule show --stage {} --id <id>` for detail (incl. note).",
        rules.len(),
        stage.name()
    );
}

// --- show ------------------------------------------------------------------

pub(crate) fn show(
    flags: &Flags,
    stage: Stage,
    rule: &Rule,
    created: Option<String>,
    updated: Option<String>,
) {
    if flags.json {
        println!("{}", to_string_pretty(&json::show(stage, rule, created, updated)).unwrap());
        return;
    }
    let style = Style::new(false);
    println!("{}", style.bold(&format!("{} #{}", stage.name(), rule.id)));
    println!();
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    if let Some(c) = rule.data.canonical() {
        rows.push(vec![Cell::text("canonical"), Cell::text(c)]);
    }
    if let Some(p) = rule.data.pattern() {
        rows.push(vec![Cell::text("pattern"), Cell::coloured(p, highlight_regex(&style, p))]);
    }
    rows.push(vec![Cell::text("note"), Cell::text(rule.data.note().unwrap_or("—"))]);
    if let Some(c) = created {
        rows.push(vec![Cell::text("created_at"), Cell::text(c)]);
    }
    if let Some(u) = updated {
        rows.push(vec![Cell::text("updated_at"), Cell::text(u)]);
    }
    print!("{}", render_table(&style, &[("field", Align::Left), ("value", Align::Left)], &rows));
}

// --- test ------------------------------------------------------------------

pub(crate) fn test(flags: &Flags, result: &TestResult, input: &str) -> Result<(), AppError> {
    if flags.json {
        let v = match result {
            TestResult::Matches { canonical, span } => json::test_match(canonical, *span),
            TestResult::Misses => json::test_miss(),
            TestResult::SyntaxError(m) => return Err(AppError::syntax(m.clone())),
        };
        println!("{v}");
        return Ok(());
    }
    let style = Style::new(false);
    match result {
        TestResult::Matches { canonical, span } => {
            let matched = span
                .map(|(s, e)| format!("        (matched span: {:?})", &input[s..e]))
                .unwrap_or_default();
            println!("{}  →  {}{}", style.green("✓ matches"), canonical, matched);
        }
        TestResult::Misses => println!("{}", style.dim("✗ no match")),
        TestResult::SyntaxError(m) => return Err(AppError::syntax(m.clone())),
    }
    Ok(())
}

// --- evaluate --------------------------------------------------------------

/// Single-width glyphs (render cleanly inside the aligned table) — colour
/// carries the meaning, the glyph is a quick visual anchor (rule-cli §14.3).
const GLYPH_GAIN: &str = "+"; // gained a match / now affected (green)
const GLYPH_MOVED: &str = "±"; // reassigned to another rule (blue)
const GLYPH_FALL: &str = "-"; // now unmatched / no longer affected (red)
const GLYPH_UNCHANGED: &str = "·"; // unchanged (dim)

pub(crate) fn evaluate(flags: &Flags, stage: Stage, mutation: &Mutation, buckets: &Buckets) {
    if flags.json {
        println!("{}", to_string_pretty(&json::evaluate(stage, mutation, buckets)).unwrap());
        return;
    }
    let style = Style::new(false);
    if !flags.quiet {
        println!("{}", style.bold("EVALUATE (dry-run — nothing written)"));
        println!(
            "{} {} {}",
            style.bold("candidate:"),
            stage.name(),
            mutation_label(&style, mutation)
        );
        println!();
    }

    // Summary table: one row per bucket; numbers right-aligned.
    let cols = [
        ("outcome", Align::Left),
        ("payees", Align::Right),
        ("txns", Align::Right),
        ("value", Align::Right),
    ];
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    // (glyph, colour tag g/b/r, bucket) for the detail table below.
    let mut sections: Vec<(&str, &str, &BucketCount)> = Vec::new();

    let mut push = |glyph: &str, label: &str, coloured_label: String, b: &BucketCount| {
        rows.push(vec![
            Cell::coloured(format!("{glyph} {label}"), format!("{glyph} {coloured_label}")),
            Cell::text(commas(b.payees)),
            Cell::text(commas(b.txns)),
            Cell::text(money(b.total_cents)),
        ]);
    };

    match buckets {
        Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => {
            push(GLYPH_GAIN, "newly matched", style.green("newly matched"), newly_matched);
            push(GLYPH_MOVED, "moved from other", style.wrap("34", "moved from other"), stolen);
            push(GLYPH_FALL, "new fallthrough", style.red("new fallthrough"), new_fallthrough);
            unchanged_row(&style, &mut rows, *unchanged_payees);
            sections.push((GLYPH_GAIN, "g", newly_matched));
            sections.push((GLYPH_MOVED, "b", stolen));
            sections.push((GLYPH_FALL, "r", new_fallthrough));
        }
        Buckets::Loop { newly_affected, no_longer_affected, unchanged_payees } => {
            push(GLYPH_GAIN, "newly affected", style.green("newly affected"), newly_affected);
            push(
                GLYPH_FALL,
                "no longer affected",
                style.red("no longer affected"),
                no_longer_affected,
            );
            unchanged_row(&style, &mut rows, *unchanged_payees);
            sections.push((GLYPH_GAIN, "g", newly_affected));
            sections.push((GLYPH_FALL, "r", no_longer_affected));
        }
    }
    print!("{}", render_table(&style, &cols, &rows));

    // Detail table of the changed payees: payee · txns · value · old · new.
    // The outcome glyph (coloured) prefixes the payee so the bucket is
    // legible with or without colour; old/new show the before/after (— = none).
    let detail_cols = [
        ("payee", Align::Left),
        ("txns", Align::Right),
        ("value", Align::Right),
        ("old", Align::Left),
        ("new", Align::Left),
    ];
    let limit = if flags.all { usize::MAX } else { SAMPLE_LIMIT };
    let mut detail: Vec<Vec<Cell>> = Vec::new();
    let mut hidden = 0usize;
    for (glyph, tag, b) in &sections {
        let take = b.samples.len().min(limit);
        hidden += b.samples.len() - take;
        for s in b.samples.iter().take(take) {
            let payee = sanitize(&s.original_payee);
            detail.push(vec![
                Cell::coloured(
                    format!("{glyph} {payee}"),
                    format!("{} {payee}", paint(&style, tag, glyph)),
                ),
                Cell::text(commas(s.txn_count)),
                Cell::text(money(s.total_cents)),
                opt_cell(&style, s.was.as_deref()),
                opt_cell(&style, s.now.as_deref()),
            ]);
        }
    }
    if !detail.is_empty() {
        println!();
        print!("{}", render_table(&style, &detail_cols, &detail));
        if hidden > 0 {
            println!("  {}", style.dim(&format!("… +{hidden} more (use --all)")));
        }
    }

    println!();
    let n = buckets.changed_payees();
    let force = if matches!(mutation, Mutation::Delete { .. }) { " --force" } else { "" };
    println!(
        "Re-run with --apply{force} to commit. {n} payees would re-stage — then run \
         `normalise` (scan) to refresh proposals."
    );
}

fn unchanged_row(style: &Style, rows: &mut Vec<Vec<Cell>>, payees: i64) {
    rows.push(vec![
        Cell::coloured(
            format!("{GLYPH_UNCHANGED} unchanged"),
            style.dim(&format!("{GLYPH_UNCHANGED} unchanged")),
        ),
        Cell::text(commas(payees)),
        Cell::coloured("—", style.dim("—")),
        Cell::coloured("—", style.dim("—")),
    ]);
}

/// Colour `s` by an outcome tag: g=green, b=blue, r=red.
fn paint(style: &Style, tag: &str, s: &str) -> String {
    match tag {
        "g" => style.green(s),
        "b" => style.wrap("34", s),
        _ => style.red(s),
    }
}

/// A cell for the old/new columns: the value (sanitised), or a dim em-dash.
fn opt_cell(style: &Style, v: Option<&str>) -> Cell {
    match v {
        Some(v) => Cell::text(sanitize(v)),
        None => Cell::coloured("—", style.dim("—")),
    }
}

fn mutation_label(style: &Style, mutation: &Mutation) -> String {
    match mutation {
        Mutation::Add(d) => format!("+add {}", labelled(style, d)),
        Mutation::Edit { id, data } => format!("~edit #{id} {}", labelled(style, data)),
        Mutation::Delete { id, .. } => format!("−delete #{id}"),
        Mutation::Move { id, target, .. } => match target {
            MoveTarget::Before(a) => format!("move #{id} before #{a}"),
            MoveTarget::After(a) => format!("move #{id} after #{a}"),
        },
    }
}

/// `"canonical"  <highlighted pattern>` for the candidate line.
fn labelled(style: &Style, d: &RuleData) -> String {
    match (d.canonical(), d.pattern()) {
        (Some(c), Some(p)) => format!("{c:?}  {}", highlight_regex(style, p)),
        (Some(c), None) => format!("{c:?}"),
        (None, Some(p)) => highlight_regex(style, p),
        (None, None) => String::new(),
    }
}

// --- apply -----------------------------------------------------------------

pub(crate) fn apply(flags: &Flags, stage: Stage, res: &CommitResult) {
    if flags.json {
        println!("{}", to_string_pretty(&json::apply(stage, res)).unwrap());
        return;
    }
    let style = Style::new(false);
    let id_suffix = res.new_id.map(|id| format!("   ({} #{id})", stage.name())).unwrap_or_default();
    println!("{} committed: {}{}", style.green("✓"), res.change, id_suffix);
    println!("{} re-dumped rules/{}.sql", style.green("✓"), stage.name());
    println!(
        "{} {} payees would re-stage — run `normalise` to refresh proposals.",
        style.yellow("⚠"),
        res.dirty_payees
    );
}
