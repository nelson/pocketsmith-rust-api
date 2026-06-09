//! `rule` — view and edit normalisation rules (rule-cli §2, §4–8).
//!
//! A thin presentation shell over the `rules` library: parse → build a
//! `Mutation` → `validate_draft` → `compute_buckets` (always) → `commit`
//! (on `--apply`) → render as text or JSON. All rule semantics live in
//! the library; this binary owns only argument parsing, colour, and the
//! text/JSON rendering (rule-cli §1.1).

use std::collections::HashMap;
use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use pocketsmith_sync::db;
use pocketsmith_sync::rules::impact::{self, BucketCount, Buckets, TestResult, SAMPLE_LIMIT};
use pocketsmith_sync::rules::model::{LocationKind, MoveTarget, Mutation, Rule, RuleData, RuleError};
use pocketsmith_sync::rules::validate::{validate_draft, StageSchema};
use pocketsmith_sync::rules::{commit, crud, rules_dir, CommitResult, DumpPolicy, Stage};

// ---------------------------------------------------------------------------
// Error type carrying an exit code + JSON-awareness (rule-cli §7, §13).
// ---------------------------------------------------------------------------

struct AppError {
    message: String,
    code: i32,
    /// `true` → render `syntax error:` rather than `error:`.
    syntax: bool,
}

impl AppError {
    fn usage(msg: impl Into<String>) -> AppError {
        AppError { message: msg.into(), code: 1, syntax: false }
    }
    fn syntax(msg: impl Into<String>) -> AppError {
        AppError { message: msg.into(), code: 2, syntax: true }
    }
}

impl From<RuleError> for AppError {
    fn from(e: RuleError) -> Self {
        AppError { message: e.to_string(), code: e.exit_code(), syntax: e.is_syntax() }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        // Map a typed RuleError carried through anyhow back to its code.
        if let Some(re) = e.downcast_ref::<RuleError>() {
            return AppError {
                message: re.to_string(),
                code: re.exit_code(),
                syntax: re.is_syntax(),
            };
        }
        AppError::usage(format!("{e:#}"))
    }
}

fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if json {
                eprintln!("{}", json!({ "error": e.message, "code": e.code }));
            } else if e.syntax {
                eprintln!("syntax error: {}", e.message);
            } else {
                eprintln!("error: {}", e.message);
            }
            ExitCode::from(e.code as u8)
        }
    }
}

fn run(args: &[String]) -> Result<(), AppError> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    let verb = args[0].as_str();
    let flags = Flags::parse(&args[1..])?;

    match verb {
        "list" => cmd_list(&flags),
        "show" => cmd_show(&flags),
        "test" => cmd_test(&flags),
        "add" => cmd_add(&flags),
        "edit" => cmd_edit(&flags),
        "rm" => cmd_rm(&flags),
        "move" => cmd_move(&flags),
        "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(AppError::usage(format!("unknown verb {other:?}. Try `rule --help`."))),
    }
}

// ---------------------------------------------------------------------------
// Argument parsing (hand-rolled, matching the repo's bin/normalise.rs style).
// ---------------------------------------------------------------------------

const VALUE_FLAGS: &[&str] =
    &["pattern", "canonical", "operation", "gateway", "institution", "kind", "note"];

struct Flags {
    stage: Option<String>,
    id: Option<i64>,
    json: bool,
    apply: bool,
    force: bool,
    quiet: bool,
    all: bool,
    before: Option<i64>,
    after: Option<i64>,
    values: HashMap<String, String>,
    features: HashMap<String, bool>,
    positionals: Vec<String>,
}

impl Flags {
    fn parse(args: &[String]) -> Result<Flags, AppError> {
        let mut f = Flags {
            stage: None,
            id: None,
            json: false,
            apply: false,
            force: false,
            quiet: false,
            all: false,
            before: None,
            after: None,
            values: HashMap::new(),
            features: HashMap::new(),
            positionals: Vec::new(),
        };
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            let take_val = |name: &str, i: &mut usize| -> Result<String, AppError> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| AppError::usage(format!("--{name} requires a value")))
            };
            match a {
                "--json" => f.json = true,
                "--apply" | "-a" => f.apply = true,
                "--force" | "-f" => f.force = true,
                "--quiet" => f.quiet = true,
                "--all" => f.all = true,
                "-af" | "-fa" => {
                    f.apply = true;
                    f.force = true;
                }
                "--stage" => f.stage = Some(take_val("stage", &mut i)?),
                "--id" => {
                    let v = take_val("id", &mut i)?;
                    f.id = Some(v.parse().map_err(|_| AppError::usage(format!("--id must be an integer, got {v:?}")))?);
                }
                "--before" => {
                    let v = take_val("before", &mut i)?;
                    f.before = Some(v.parse().map_err(|_| AppError::usage(format!("--before must be an integer, got {v:?}")))?);
                }
                "--after" => {
                    let v = take_val("after", &mut i)?;
                    f.after = Some(v.parse().map_err(|_| AppError::usage(format!("--after must be an integer, got {v:?}")))?);
                }
                _ if a.starts_with("--has-") => {
                    let feat = a.trim_start_matches("--has-").to_string();
                    f.features.insert(feat, true);
                }
                _ if a.starts_with("--no-") => {
                    // --no-currency-code etc. Normalise to the feature stem.
                    let feat = a.trim_start_matches("--no-").to_string();
                    f.features.insert(feat, false);
                }
                _ if a.starts_with("--") => {
                    let name = a.trim_start_matches("--").to_string();
                    if VALUE_FLAGS.contains(&name.as_str()) {
                        let v = take_val(&name, &mut i)?;
                        f.values.insert(name, v);
                    } else {
                        return Err(AppError::usage(format!("unknown flag {a}")));
                    }
                }
                _ => f.positionals.push(a.to_string()),
            }
            i += 1;
        }
        Ok(f)
    }

    fn stage(&self) -> Result<Stage, AppError> {
        let name = self
            .stage
            .as_deref()
            .ok_or_else(|| AppError::usage("--stage <name> is required"))?;
        Stage::from_name(name).ok_or_else(|| RuleError::UnknownStage(name.to_string()).into())
    }

    fn require_id(&self) -> Result<i64, AppError> {
        self.id.ok_or_else(|| AppError::usage("--id <id> is required"))
    }
}

// ---------------------------------------------------------------------------
// Colour helper (rule-cli §4.0): on for a TTY, off when piped/--json/NO_COLOR.
// ---------------------------------------------------------------------------

struct Style {
    on: bool,
}

impl Style {
    fn new(json: bool) -> Style {
        let on = !json
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal();
        Style { on }
    }
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
}

// ---------------------------------------------------------------------------
// Helpers shared by the commands.
// ---------------------------------------------------------------------------

fn open_db() -> Result<Connection, AppError> {
    db::open_app_db().map_err(AppError::from)
}

/// Format cents as "$8.4k" / "$980" (magnitude, one-dp k for ≥ $1000).
fn money(cents: i64) -> String {
    let dollars = cents as f64 / 100.0;
    if dollars >= 1000.0 {
        format!("${:.1}k", dollars / 1000.0)
    } else {
        format!("${:.0}", dollars)
    }
}

/// Group an integer with thousands separators: 1204 → "1,204".
fn commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

// ---------------------------------------------------------------------------
// Aligned table renderer. Column widths come from the *plain* text so ANSI
// colour codes in the shown cell never throw off alignment.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
}

/// One cell: `plain` drives width/alignment, `shown` is what's printed
/// (may carry ANSI). For uncoloured cells the two are equal.
struct Cell {
    plain: String,
    shown: String,
}

impl Cell {
    fn text(s: impl Into<String>) -> Cell {
        let s = s.into();
        Cell { plain: s.clone(), shown: s }
    }
    fn coloured(plain: impl Into<String>, shown: impl Into<String>) -> Cell {
        Cell { plain: plain.into(), shown: shown.into() }
    }
}

/// Render an aligned table: bold headers, a rule separator, then rows.
/// Each line is indented two spaces to match the rest of the CLI.
fn render_table(style: &Style, headers: &[(&str, Align)], rows: &[Vec<Cell>]) -> String {
    let ncol = headers.len();
    let mut widths = vec![0usize; ncol];
    for (i, (h, _)) in headers.iter().enumerate() {
        widths[i] = h.chars().count();
    }
    for row in rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.plain.chars().count());
        }
    }
    let pad = |plain_len: usize, width: usize, align: Align, shown: &str| -> String {
        let gap = width.saturating_sub(plain_len);
        match align {
            Align::Left => format!("{shown}{}", " ".repeat(gap)),
            Align::Right => format!("{}{shown}", " ".repeat(gap)),
        }
    };
    let mut out = String::new();
    // Header row.
    let head: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, (h, a))| style.bold(&pad(h.chars().count(), widths[i], *a, h)))
        .collect();
    out.push_str(&format!("  {}\n", head.join("  ")));
    // Separator.
    let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    out.push_str(&format!("  {}\n", sep.join("  ")));
    // Body.
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| pad(c.plain.chars().count(), widths[i], headers[i].1, &c.shown))
            .collect();
        out.push_str(&format!("  {}\n", cells.join("  ").trim_end()));
    }
    out
}

// ---------------------------------------------------------------------------
// Regex syntax highlighting (rule-cli feedback B): make anchors, groups,
// quantifiers, character classes, escapes and literals distinguishable.
// ---------------------------------------------------------------------------

/// Colourise a regex `pattern` for display. Returns the plain pattern
/// unchanged when colour is off (so it stays a faithful, copyable cell).
/// Colourise a regex `pattern` for display. Returns the plain pattern
/// unchanged when colour is off (so it stays a faithful, copyable cell).
///
/// Scheme (rule-cli feedback B):
///   - grouping brackets `( )` → dim grey (structure, de-emphasised)
///   - group constructs `?i` / `?:` / `?P<name>` → blue
///   - every other regex special (`\b \d ^ $ [ ] * + ? { } . |`) → blue
///   - the literal text you're actually matching → **bold green**
fn highlight_regex(style: &Style, pattern: &str) -> String {
    if !style.on {
        return pattern.to_string();
    }
    const GREY: &str = "90";
    const BLUE: &str = "34";
    const LITERAL: &str = "1;32"; // bold green
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Escape sequence: backslash + the following char (e.g. \b \d \*).
            '\\' => {
                let mut esc = String::from('\\');
                if let Some(n) = chars.next() {
                    esc.push(n);
                }
                out.push_str(&style.wrap(BLUE, &esc));
            }
            // Opening bracket: grey. If it begins a group construct
            // (`(?...`), colour the construct prefix blue.
            '(' => {
                out.push_str(&style.wrap(GREY, "("));
                if chars.peek() == Some(&'?') {
                    let mut con = String::from(chars.next().unwrap()); // '?'
                    if matches!(chars.peek(), Some('P') | Some('<')) {
                        // Named group: consume through the closing '>'.
                        while let Some(&n) = chars.peek() {
                            con.push(n);
                            chars.next();
                            if n == '>' {
                                break;
                            }
                        }
                    } else {
                        // Inline flags / non-capturing: flag letters (+ '-'),
                        // optionally terminated by ':' (the ')' is emitted grey
                        // by the loop on the next iteration).
                        while let Some(&n) = chars.peek() {
                            if n.is_ascii_alphabetic() || n == '-' {
                                con.push(n);
                                chars.next();
                            } else if n == ':' {
                                con.push(n);
                                chars.next();
                                break;
                            } else {
                                break;
                            }
                        }
                    }
                    out.push_str(&style.wrap(BLUE, &con));
                }
            }
            ')' => out.push_str(&style.wrap(GREY, ")")),
            // Every other regex special: blue.
            '^' | '$' | '[' | ']' | '*' | '+' | '?' | '{' | '}' | '.' | '|' => {
                out.push_str(&style.wrap(BLUE, &c.to_string()))
            }
            // The literal text being matched for — bold green.
            other => out.push_str(&style.wrap(LITERAL, &other.to_string())),
        }
    }
    out
}

/// Build a `RuleData` for `stage` from the flags, inheriting from `base`
/// (the saved rule, for `edit`) when a value/feature isn't given.
fn build_rule_data(
    stage: Stage,
    flags: &Flags,
    base: Option<&RuleData>,
) -> Result<RuleData, AppError> {
    let schema = StageSchema::for_stage(stage);
    // Reject out-of-schema flags up front (rule-cli §2.2).
    for k in flags.values.keys() {
        if !schema.allows_value(k) {
            return Err(RuleError::UnknownFlag { stage, flag: k.clone() }.into());
        }
    }
    for k in flags.features.keys() {
        if !schema.allows_feature(k) {
            return Err(RuleError::UnknownFlag { stage, flag: format!("has-{k}") }.into());
        }
    }

    // Value getter: explicit flag, else the saved value, else None.
    let val = |name: &str, base_val: Option<&str>| -> Option<String> {
        flags.values.get(name).cloned().or_else(|| base_val.map(|s| s.to_string()))
    };
    let req = |name: &str, base_val: Option<&str>| -> String {
        val(name, base_val).unwrap_or_default()
    };
    // Feature getter: explicit toggle, else inherited (add → false).
    let feat = |name: &str, base_default: bool| -> bool {
        flags.features.get(name).copied().unwrap_or(base_default)
    };

    macro_rules! base_field {
        ($variant:path { $field:ident }) => {
            match base {
                Some($variant { $field, .. }) => Some($field.as_str()),
                _ => None,
            }
        };
    }
    macro_rules! base_opt {
        ($variant:path { $field:ident }) => {
            match base {
                Some($variant { $field: Some(v), .. }) => Some(v.as_str()),
                _ => None,
            }
        };
    }
    macro_rules! base_flag {
        ($variant:path { $field:ident }) => {
            matches!(base, Some($variant { $field: true, .. }))
        };
    }

    let data = match stage {
        Stage::Prefixes => RuleData::Prefix {
            pattern: req("pattern", base_field!(RuleData::Prefix { pattern })),
            gateway: val("gateway", base_opt!(RuleData::Prefix { gateway })),
            operation: val("operation", base_opt!(RuleData::Prefix { operation })),
            has_account: feat("account", base_flag!(RuleData::Prefix { has_account })),
            has_date: feat("date", base_flag!(RuleData::Prefix { has_date })),
            note: val("note", base_opt!(RuleData::Prefix { note })),
        },
        Stage::Suffixes => RuleData::Suffix {
            pattern: req("pattern", base_field!(RuleData::Suffix { pattern })),
            gateway: val("gateway", base_opt!(RuleData::Suffix { gateway })),
            operation: val("operation", base_opt!(RuleData::Suffix { operation })),
            institution: val("institution", base_opt!(RuleData::Suffix { institution })),
            has_account: feat("account", base_flag!(RuleData::Suffix { has_account })),
            has_date: feat("date", base_flag!(RuleData::Suffix { has_date })),
            has_location: feat("location", base_flag!(RuleData::Suffix { has_location })),
            has_currency_code: feat("currency-code", base_flag!(RuleData::Suffix { has_currency_code })),
            has_amount: feat("amount", base_flag!(RuleData::Suffix { has_amount })),
            note: val("note", base_opt!(RuleData::Suffix { note })),
        },
        Stage::Expansions => RuleData::Expansion {
            pattern: req("pattern", base_field!(RuleData::Expansion { pattern })),
            canonical: req("canonical", base_field!(RuleData::Expansion { canonical })),
            note: val("note", base_opt!(RuleData::Expansion { note })),
        },
        Stage::Persons => RuleData::Person {
            canonical: req("canonical", base_field!(RuleData::Person { canonical })),
            pattern: req("pattern", base_field!(RuleData::Person { pattern })),
            note: val("note", base_opt!(RuleData::Person { note })),
        },
        Stage::Employers => RuleData::Employer {
            canonical: req("canonical", base_field!(RuleData::Employer { canonical })),
            pattern: req("pattern", base_field!(RuleData::Employer { pattern })),
            note: val("note", base_opt!(RuleData::Employer { note })),
        },
        Stage::Merchants => RuleData::Merchant {
            canonical: req("canonical", base_field!(RuleData::Merchant { canonical })),
            pattern: req("pattern", base_field!(RuleData::Merchant { pattern })),
            note: val("note", base_opt!(RuleData::Merchant { note })),
        },
        Stage::BankingOps => RuleData::BankingOp {
            operation: req("operation", base_field!(RuleData::BankingOp { operation })),
            pattern: req("pattern", base_field!(RuleData::BankingOp { pattern })),
            has_account: feat("account", base_flag!(RuleData::BankingOp { has_account })),
            note: val("note", base_opt!(RuleData::BankingOp { note })),
        },
        Stage::Locations => {
            let kind_str = val("kind", base.and_then(|b| match b {
                RuleData::Location { kind, .. } => Some(kind.as_str()),
                _ => None,
            }))
            .unwrap_or_else(|| "location".to_string());
            let kind = LocationKind::from_str(&kind_str)
                .ok_or_else(|| RuleError::BadKind(kind_str.clone()))?;
            RuleData::Location {
                location: req("canonical", base_field!(RuleData::Location { location })),
                kind,
                note: val("note", base_opt!(RuleData::Location { note })),
            }
        }
    };
    Ok(data)
}

// ---------------------------------------------------------------------------
// Commands — reads.
// ---------------------------------------------------------------------------

fn cmd_list(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let conn = open_db()?;
    let rules = crud::list(&conn, stage)?;
    if flags.json {
        let ordered = crud::is_movable(stage);
        let arr: Vec<Value> = rules.iter().map(|r| rule_summary_json(r)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "stage": stage.name(),
                "ordered": ordered,
                "rules": arr,
            }))
            .unwrap()
        );
        return Ok(());
    }
    let style = Style::new(false);
    render_list_text(&style, stage, &rules);
    Ok(())
}

fn rule_summary_json(r: &Rule) -> Value {
    json!({
        "id": r.id,
        "canonical": r.data.canonical(),
        "pattern": r.data.pattern(),
        "note": r.data.note(),
    })
}

fn render_list_text(style: &Style, stage: Stage, rules: &[Rule]) {
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
                    Cell::coloured(pat, highlight_regex(style, pat)),
                ]
            })
            .collect();
        render_table(style, &cols, &rows)
    } else {
        let cols = [("id", Align::Right), ("canonical", Align::Left), ("pattern", Align::Left)];
        let rows: Vec<Vec<Cell>> = rules
            .iter()
            .map(|r| {
                let pat = r.data.pattern().unwrap_or("—");
                vec![
                    Cell::text(r.id.to_string()),
                    Cell::text(r.data.canonical().unwrap_or("—")),
                    Cell::coloured(pat, highlight_regex(style, pat)),
                ]
            })
            .collect();
        render_table(style, &cols, &rows)
    };
    print!("{table}");
    println!();
    println!(
        "{} rules.  `rule show --stage {} --id <id>` for detail (incl. note).",
        rules.len(),
        stage.name()
    );
}

fn cmd_show(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    let rule = crud::get(&conn, stage, id)?
        .ok_or(RuleError::NotFound { stage, id })?;
    let (created, updated) = timestamps(&conn, stage, id);
    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "stage": stage.name(),
                "id": rule.id,
                "canonical": rule.data.canonical(),
                "pattern": rule.data.pattern(),
                "note": rule.data.note(),
                "created_at": created,
                "updated_at": updated,
            }))
            .unwrap()
        );
        return Ok(());
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
    Ok(())
}

/// Best-effort created_at/updated_at lookup (not part of the typed Rule).
fn timestamps(conn: &Connection, stage: Stage, id: i64) -> (Option<String>, Option<String>) {
    let sql = format!("SELECT created_at, updated_at FROM {} WHERE id = ?1", stage.table());
    conn.query_row(&sql, [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((None, None))
}

fn cmd_test(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let input = flags
        .positionals
        .first()
        .ok_or_else(|| AppError::usage("a test string positional is required"))?;
    let candidate = build_rule_data(stage, flags, None)?;
    let conn = open_db()?;
    let result = impact::test_one(&conn, stage, &candidate, input);
    if flags.json {
        let v = match &result {
            TestResult::Matches { canonical, span } => json!({
                "matches": true, "canonical": canonical, "span": span.map(|(s,e)| [s,e]),
            }),
            TestResult::Misses => json!({ "matches": false }),
            TestResult::SyntaxError(m) => {
                return Err(AppError::syntax(m.clone()));
            }
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
        TestResult::SyntaxError(m) => return Err(AppError::syntax(m)),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands — evaluate / apply.
// ---------------------------------------------------------------------------

fn cmd_add(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let data = build_rule_data(stage, flags, None)?;
    validate_draft(&data)?;
    let mutation = Mutation::Add(data);
    evaluate_or_apply(flags, stage, mutation)
}

fn cmd_edit(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    let existing = crud::get(&conn, stage, id)?.ok_or(RuleError::NotFound { stage, id })?;
    let data = build_rule_data(stage, flags, Some(&existing.data))?;
    validate_draft(&data)?;
    let mutation = Mutation::Edit { id, data };
    evaluate_or_apply_conn(conn, flags, stage, mutation)
}

fn cmd_rm(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    // Must exist to evaluate/apply.
    crud::get(&conn, stage, id)?.ok_or(RuleError::NotFound { stage, id })?;
    let mutation = Mutation::Delete { stage, id };
    if flags.apply && !flags.force {
        return Err(AppError::usage("deleting a rule requires --force (-f)"));
    }
    evaluate_or_apply_conn(conn, flags, stage, mutation)
}

fn cmd_move(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let target = match (flags.before, flags.after) {
        (Some(a), None) => MoveTarget::Before(a),
        (None, Some(a)) => MoveTarget::After(a),
        (None, None) => return Err(AppError::usage("move requires --before <id> or --after <id>")),
        (Some(_), Some(_)) => return Err(AppError::usage("use only one of --before / --after")),
    };
    let mutation = Mutation::Move { stage, id, target };
    evaluate_or_apply(flags, stage, mutation)
}

fn evaluate_or_apply(flags: &Flags, stage: Stage, mutation: Mutation) -> Result<(), AppError> {
    let conn = open_db()?;
    evaluate_or_apply_conn(conn, flags, stage, mutation)
}

fn evaluate_or_apply_conn(
    conn: Connection,
    flags: &Flags,
    stage: Stage,
    mutation: Mutation,
) -> Result<(), AppError> {
    if flags.apply {
        let res =
            commit::commit(&conn, &mutation, DumpPolicy::Sync(rules_dir()), None)?;
        render_apply(flags, stage, &res);
        Ok(())
    } else {
        let payees = impact::load_payees(&conn)?;
        let buckets = impact::compute_buckets(&conn, stage, &mutation, &payees)?;
        render_evaluate(flags, stage, &mutation, &buckets);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rendering — evaluate.
// ---------------------------------------------------------------------------

fn render_evaluate(flags: &Flags, stage: Stage, mutation: &Mutation, buckets: &Buckets) {
    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&evaluate_json(stage, mutation, buckets)).unwrap()
        );
        return;
    }
    let style = Style::new(false);
    if !flags.quiet {
        println!("{}", style.bold("EVALUATE (dry-run — nothing written)"));
        println!("{} {} {}", style.bold("candidate:"), stage.name(), mutation_label(&style, mutation));
        println!();
    }

    // One summary row per bucket; numbers right-aligned, glyph+label coloured.
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
            push(GLYPH_FALL, "no longer affected", style.red("no longer affected"), no_longer_affected);
            unchanged_row(&style, &mut rows, *unchanged_payees);
            sections.push((GLYPH_GAIN, "g", newly_affected));
            sections.push((GLYPH_FALL, "r", no_longer_affected));
        }
    }
    print!("{}", render_table(&style, &cols, &rows));

    // Detail table of the changed payees, one aligned grid across all
    // buckets: payee · txns · value · old · new. The outcome glyph (coloured)
    // prefixes the payee so the bucket is legible with or without colour;
    // `old`/`new` show the canonical before/after (— = none).
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

/// Single-width glyphs (render cleanly inside the aligned table) — colour
/// carries the meaning, the glyph is a quick visual anchor.
const GLYPH_GAIN: &str = "+"; // gained a match / now affected (green)
const GLYPH_MOVED: &str = "±"; // reassigned to another rule (blue)
const GLYPH_FALL: &str = "-"; // now unmatched / no longer affected (red)
const GLYPH_UNCHANGED: &str = "·"; // unchanged (dim)

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

/// Strip control characters (newlines, tabs, etc.) from a payee and
/// collapse runs of whitespace, so a multi-line bank payee stays on one
/// aligned table row.
fn sanitize(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Colour `s` by an outcome tag: g=green, b=blue, r=red.
fn paint(style: &Style, tag: &str, s: &str) -> String {
    match tag {
        "g" => style.green(s),
        "b" => style.wrap("34", s),
        _ => style.red(s),
    }
}

/// A canonical cell for the old/new columns: the value (sanitised), or a
/// dim em-dash when absent.
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

fn evaluate_json(stage: Stage, mutation: &Mutation, buckets: &Buckets) -> Value {
    let buckets_json = match buckets {
        Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => json!({
            "newly_matched": bucket_json(newly_matched, true),
            "stolen": bucket_json(stolen, true),
            "new_fallthrough": bucket_json(new_fallthrough, true),
            "unchanged": { "payees": unchanged_payees },
        }),
        Buckets::Loop { newly_affected, no_longer_affected, unchanged_payees } => json!({
            "newly_affected": bucket_json(newly_affected, true),
            "no_longer_affected": bucket_json(no_longer_affected, true),
            "unchanged": { "payees": unchanged_payees },
        }),
    };
    json!({
        "mode": "evaluate",
        "committed": false,
        "stage": stage.name(),
        "mutation": mutation_json(mutation),
        "buckets": buckets_json,
        "dirty_payees": buckets.changed_payees(),
    })
}

fn bucket_json(b: &BucketCount, with_samples: bool) -> Value {
    let samples: Vec<Value> = if with_samples {
        b.samples
            .iter()
            .map(|s| {
                json!({
                    "original_payee": s.original_payee,
                    "txns": s.txn_count,
                    "total_cents": s.total_cents,
                    "account": s.account,
                    "was": s.was,
                    "now": s.now,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    json!({ "payees": b.payees, "txns": b.txns, "total_cents": b.total_cents, "samples": samples })
}

fn mutation_json(mutation: &Mutation) -> Value {
    match mutation {
        Mutation::Add(d) => {
            json!({ "kind": "add", "canonical": d.canonical(), "pattern": d.pattern() })
        }
        Mutation::Edit { id, data } => json!({
            "kind": "edit", "id": id, "canonical": data.canonical(), "pattern": data.pattern(),
        }),
        Mutation::Delete { id, .. } => json!({ "kind": "delete", "id": id }),
        Mutation::Move { id, target, .. } => {
            let (k, a) = match target {
                MoveTarget::Before(a) => ("before", a),
                MoveTarget::After(a) => ("after", a),
            };
            json!({ "kind": "move", "id": id, k: a })
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering — apply.
// ---------------------------------------------------------------------------

fn render_apply(flags: &Flags, stage: Stage, res: &CommitResult) {
    if flags.json {
        let mut obj = json!({
            "mode": "apply",
            "committed": true,
            "new_id": res.new_id,
            "stage": stage.name(),
            "change": res.change,
            "dumped": format!("rules/{}.sql", stage.name()),
            "dirty_payees": res.dirty_payees,
        });
        if res.new_id.is_none() {
            obj.as_object_mut().unwrap().remove("new_id");
        }
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
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

// ---------------------------------------------------------------------------

fn print_help() {
    println!(
        "rule — view and edit normalisation rules (scriptable; library-backed)\n\
\n\
USAGE\n  \
  rule list   --stage <stage> [--json]\n  \
  rule show   --stage <stage> --id <id> [--json]\n  \
  rule test   --stage <stage> [candidate flags] \"<string>\"\n  \
  rule add    --stage <stage> [field flags] [--apply|-a] [--json]\n  \
  rule edit   --stage <stage> --id <id> [field flags] [--apply|-a] [--json]\n  \
  rule rm     --stage <stage> --id <id> [--apply --force | -af] [--json]\n  \
  rule move   --stage <stage> --id <id> (--before <id> | --after <id>) [--apply|-a]\n\
\n\
STAGES\n  \
  prefixes suffixes expansions  (loop — ordered, reorder with `move`)\n  \
  persons employers merchants banking_ops  (first-match-wins, auto-ordered)\n  \
  locations  (additive)\n\
\n\
DEFAULTS\n  \
  add/edit/rm/move are DRY-RUN (evaluate) unless --apply/-a is given.\n  \
  rm additionally requires --force/-f to commit (combine as -af).\n\
\n\
FIELD FLAGS (validated per stage; --stage is always required)\n  \
  values:   --pattern --canonical --operation --gateway --institution --kind --note\n  \
  features: --has-account --has-date --has-location --has-currency-code --has-amount\n            \
  (negatives --no-* only meaningful on `edit`; a --has-* feature\n             \
  requires the matching (?P<name>...) capture group in --pattern)\n\
\n\
Colour/bold on a TTY; plain when piped, --json, or NO_COLOR set."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // Force colour on (bypasses the TTY check) so the scheme is testable.
    fn styled() -> Style {
        Style { on: true }
    }

    #[test]
    fn regex_highlight_colours_brackets_flags_literals_and_specials() {
        let s = styled();
        let out = highlight_regex(&s, r"(?i)ZAP\b");
        // brackets dim grey (90)
        assert!(out.contains("\x1b[90m(\x1b[0m"), "open bracket grey: {out:?}");
        assert!(out.contains("\x1b[90m)\x1b[0m"), "close bracket grey");
        // inline flag ?i blue (34)
        assert!(out.contains("\x1b[34m?i\x1b[0m"), "flags blue");
        // literal letters bold green (1;32)
        assert!(out.contains("\x1b[1;32mZ\x1b[0m"), "literal bold green");
        // escape \b blue
        assert!(out.contains("\x1b[34m\\b\x1b[0m"), "escape blue");
    }

    #[test]
    fn regex_highlight_is_plain_when_colour_off() {
        let s = Style { on: false };
        assert_eq!(highlight_regex(&s, r"(?i)ZAP\b"), r"(?i)ZAP\b");
    }

    #[test]
    fn named_group_prefix_is_blue() {
        let s = styled();
        let out = highlight_regex(&s, r"(?P<account>\d+)");
        assert!(out.contains("\x1b[34m?P<account>\x1b[0m"), "named-group prefix blue: {out:?}");
    }
}
