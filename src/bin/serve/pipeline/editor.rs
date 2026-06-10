//! The parameterised Edit / Evaluate / New editor card (editable-rules-ui
//! §3.4). One renderer drives every stage; the per-stage differences come
//! from [`fields_for`] (which inputs to show) so adding a stage editor is
//! "add its field list". The card is the right column of the two-column
//! stage detail; the rule list (left column) lives in `views.rs`.
//!
//! The three modes map onto the mockups:
//!   * **Edit** — enabled inputs; actions `[E] Evaluate · [N] Cancel ·
//!     Delete`. No Save (you never save un-evaluated).
//!   * **New** — Edit with a green border + no Delete (nothing to delete).
//!   * **Evaluate** — read-only field display + hidden inputs (so Save
//!     re-posts the same values), the tester + impact buckets, and actions
//!     `[Y] Save · [B] Back to edit` (+ Delete for an existing rule).

use std::collections::HashMap;

use maud::{html, Markup};

use pocketsmith_sync::normalise::BankingOperation;
use pocketsmith_sync::rules::model::RuleData;
use pocketsmith_sync::rules::Stage;

use super::regex_hl;

/// Which editor state the card renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Edit,
    New,
    Evaluate,
    /// Evaluate the *deletion* of a rule: read-only fields + the impact of
    /// removing it, with a mouse-only `Confirm delete`. Reached only via
    /// the Edit-mode Delete button, so a rule is never deleted blindly.
    EvaluateDelete,
}

/// Everything the card needs to render. `eval_body` (tester + impact
/// buckets) is built by `impact.rs` and only shown in [`Mode::Evaluate`].
pub struct Card<'a> {
    pub stage: Stage,
    pub mode: Mode,
    /// `Some` for an existing rule (edit/evaluate of a saved rule),
    /// `None` for a brand-new rule.
    pub id: Option<i64>,
    pub data: &'a RuleData,
    /// A validation / regex-syntax error. When present, Save is disabled.
    pub error: Option<&'a str>,
    /// Tester result + impact buckets (Evaluate mode only).
    pub eval_body: Markup,
}

/// A single editor input.
struct Field {
    /// Form field name == DB column.
    name: &'static str,
    label: &'static str,
    kind: FieldKind,
    required: bool,
}

enum FieldKind {
    /// Plain text.
    Text,
    /// Regex source: monospace, with a hint about case-insensitive apply.
    Regex,
    /// Dropdown. `blank` allows an empty (None) choice.
    Select { options: Vec<&'static str>, blank: bool },
    /// Capture-feature checkbox.
    Flag,
}

/// The editable inputs for a stage, in display order. `note` is handled
/// separately (collapsible), so it never appears here.
fn fields_for(stage: Stage) -> Vec<Field> {
    let ops = || BankingOperation::all().iter().map(|o| o.display_name()).collect::<Vec<_>>();
    match stage {
        Stage::Prefixes => vec![
            Field { name: "pattern", label: "Pattern", kind: FieldKind::Regex, required: true },
            Field { name: "gateway", label: "Gateway", kind: FieldKind::Text, required: false },
            Field {
                name: "operation",
                label: "Operation",
                kind: FieldKind::Select { options: ops(), blank: true },
                required: false,
            },
            Field { name: "has_account", label: "Captures account", kind: FieldKind::Flag, required: false },
            Field { name: "has_date", label: "Captures date", kind: FieldKind::Flag, required: false },
        ],
        Stage::Suffixes => vec![
            Field { name: "pattern", label: "Pattern", kind: FieldKind::Regex, required: true },
            Field { name: "gateway", label: "Gateway", kind: FieldKind::Text, required: false },
            Field {
                name: "operation",
                label: "Operation",
                kind: FieldKind::Select { options: ops(), blank: true },
                required: false,
            },
            Field { name: "institution", label: "Institution", kind: FieldKind::Text, required: false },
            Field { name: "has_account", label: "Captures account", kind: FieldKind::Flag, required: false },
            Field { name: "has_date", label: "Captures date", kind: FieldKind::Flag, required: false },
            Field { name: "has_location", label: "Captures location", kind: FieldKind::Flag, required: false },
            Field { name: "has_currency_code", label: "Captures currency", kind: FieldKind::Flag, required: false },
            Field { name: "has_amount", label: "Captures amount", kind: FieldKind::Flag, required: false },
        ],
        Stage::Expansions => vec![
            Field { name: "pattern", label: "Pattern", kind: FieldKind::Text, required: true },
            Field { name: "canonical", label: "Canonical", kind: FieldKind::Text, required: true },
        ],
        Stage::Persons | Stage::Employers | Stage::Merchants => vec![
            Field { name: "canonical", label: "Canonical", kind: FieldKind::Text, required: true },
            Field { name: "pattern", label: "Pattern", kind: FieldKind::Regex, required: true },
        ],
        Stage::BankingOps => vec![
            Field {
                name: "operation",
                label: "Operation",
                kind: FieldKind::Select { options: ops(), blank: false },
                required: true,
            },
            Field { name: "pattern", label: "Pattern", kind: FieldKind::Regex, required: true },
            Field { name: "has_account", label: "Captures account", kind: FieldKind::Flag, required: false },
        ],
        Stage::Locations => vec![
            Field { name: "location", label: "Location", kind: FieldKind::Text, required: true },
            Field {
                name: "kind",
                label: "Kind",
                kind: FieldKind::Select { options: vec!["location", "region"], blank: false },
                required: true,
            },
        ],
    }
}

/// Text/select values of a rule keyed by field name.
fn text_values(data: &RuleData) -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    let mut put = |k: &'static str, v: Option<&str>| {
        if let Some(v) = v {
            m.insert(k, v.to_string());
        }
    };
    match data {
        RuleData::Prefix { pattern, gateway, operation, .. } => {
            put("pattern", Some(pattern));
            put("gateway", gateway.as_deref());
            put("operation", operation.as_deref());
        }
        RuleData::Suffix { pattern, gateway, operation, institution, .. } => {
            put("pattern", Some(pattern));
            put("gateway", gateway.as_deref());
            put("operation", operation.as_deref());
            put("institution", institution.as_deref());
        }
        RuleData::Expansion { pattern, canonical, .. } => {
            put("pattern", Some(pattern));
            put("canonical", Some(canonical));
        }
        RuleData::Person { canonical, pattern, .. }
        | RuleData::Employer { canonical, pattern, .. }
        | RuleData::Merchant { canonical, pattern, .. } => {
            put("canonical", Some(canonical));
            put("pattern", Some(pattern));
        }
        RuleData::BankingOp { operation, pattern, .. } => {
            put("operation", Some(operation));
            put("pattern", Some(pattern));
        }
        RuleData::Location { location, kind, .. } => {
            put("location", Some(location));
            put("kind", Some(kind.as_str()));
        }
    }
    m
}

/// Enabled capture flags of a rule keyed by field name.
fn flag_values(data: &RuleData) -> HashMap<&'static str, bool> {
    let mut m = HashMap::new();
    match data {
        RuleData::Prefix { has_account, has_date, .. } => {
            m.insert("has_account", *has_account);
            m.insert("has_date", *has_date);
        }
        RuleData::Suffix {
            has_account,
            has_date,
            has_location,
            has_currency_code,
            has_amount,
            ..
        } => {
            m.insert("has_account", *has_account);
            m.insert("has_date", *has_date);
            m.insert("has_location", *has_location);
            m.insert("has_currency_code", *has_currency_code);
            m.insert("has_amount", *has_amount);
        }
        RuleData::BankingOp { has_account, .. } => {
            m.insert("has_account", *has_account);
        }
        _ => {}
    }
    m
}

/// Base URL for this stage's editor endpoints.
fn base(stage: Stage) -> String {
    format!("/pipeline/stage/{}", stage.name())
}

/// A blank [`RuleData`] for `stage` — every text field empty, flags off —
/// used to prefill the New-rule card.
pub fn empty(stage: Stage) -> RuleData {
    use pocketsmith_sync::rules::model::LocationKind;
    match stage {
        Stage::Prefixes => RuleData::Prefix {
            pattern: String::new(),
            gateway: None,
            operation: None,
            has_account: false,
            has_date: false,
            note: None,
        },
        Stage::Suffixes => RuleData::Suffix {
            pattern: String::new(),
            gateway: None,
            operation: None,
            institution: None,
            has_account: false,
            has_date: false,
            has_location: false,
            has_currency_code: false,
            has_amount: false,
            note: None,
        },
        Stage::Expansions => RuleData::Expansion {
            pattern: String::new(),
            canonical: String::new(),
            note: None,
        },
        Stage::Persons => RuleData::Person { canonical: String::new(), pattern: String::new(), note: None },
        Stage::Employers => RuleData::Employer { canonical: String::new(), pattern: String::new(), note: None },
        Stage::Merchants => RuleData::Merchant { canonical: String::new(), pattern: String::new(), note: None },
        Stage::BankingOps => RuleData::BankingOp {
            operation: String::new(),
            pattern: String::new(),
            has_account: false,
            note: None,
        },
        Stage::Locations => RuleData::Location {
            location: String::new(),
            kind: LocationKind::Location,
            note: None,
        },
    }
}

/// Render the editor card for `card`.
pub fn render(card: &Card) -> Markup {
    let texts = text_values(card.data);
    let flags = flag_values(card.data);
    let fields = fields_for(card.stage);
    let readonly = matches!(card.mode, Mode::Evaluate | Mode::EvaluateDelete);

    let (title, pill, mode_class) = match card.mode {
        Mode::Edit => ("Edit rule", "edit mode", "mode-edit"),
        Mode::New => (
            if card.id.is_some() { "Edit rule" } else { "New rule" },
            "new rule",
            "mode-new",
        ),
        Mode::Evaluate => ("Edit rule", "evaluate mode", "mode-eval"),
        Mode::EvaluateDelete => ("Delete rule", "confirm delete", "mode-del"),
    };

    let note = card.data.note().unwrap_or("");

    html! {
        div.editor-card.(mode_class) {
            h2 {
                (title)
                span.mode-pill { (pill) }
                span.spinner.htmx-indicator #card-spin { "\u{27f3}" }
            }
            form #rule-form {
                div.editor-grid {
                    @for f in &fields {
                        (render_field(f, &texts, &flags, readonly))
                    }
                }
                (render_note(note, readonly))

                @if let Some(err) = card.error {
                    div.editor-error { (err) }
                }

                (card.eval_body)

                (render_actions(card))
            }
        }
    }
}

/// One labelled field row. In evaluate mode the value is shown read-only
/// and re-embedded as a hidden input so Save re-posts it unchanged.
fn render_field(
    f: &Field,
    texts: &HashMap<&'static str, String>,
    flags: &HashMap<&'static str, bool>,
    readonly: bool,
) -> Markup {
    let val = texts.get(f.name).cloned().unwrap_or_default();
    html! {
        label for=(format!("r-{}", f.name)) { (f.label) }
        @match &f.kind {
            FieldKind::Flag => {
                @let on = flags.get(f.name).copied().unwrap_or(false);
                @if readonly {
                    span.read-only-val { (if on { "\u{2713} yes" } else { "\u{2014} no" }) }
                    @if on { input type="hidden" name=(f.name) value="on"; }
                } @else {
                    label.flag-check {
                        input type="checkbox" id=(format!("r-{}", f.name)) name=(f.name) checked[on];
                        " " (f.label)
                    }
                }
            }
            FieldKind::Select { options, blank } => {
                @if readonly {
                    span.read-only-val { (if val.is_empty() { "\u{2014}" } else { &val }) }
                    @if !val.is_empty() { input type="hidden" name=(f.name) value=(val); }
                } @else {
                    select id=(format!("r-{}", f.name)) name=(f.name) {
                        @if *blank { option value="" selected[val.is_empty()] { "\u{2014}" } }
                        @for opt in options {
                            option value=(opt) selected[(*opt == val)] { (opt) }
                        }
                    }
                }
            }
            kind => {
                @let mono = matches!(kind, FieldKind::Regex);
                @if readonly {
                    span.read-only-val.(if mono { "mono" } else { "" }) {
                        @if val.is_empty() {
                            "\u{2014}"
                        } @else if mono {
                            (regex_hl::highlight(&val))
                        } @else {
                            (val)
                        }
                    }
                    input type="hidden" name=(f.name) value=(val);
                } @else {
                    input
                        type="text"
                        id=(format!("r-{}", f.name))
                        name=(f.name)
                        class=(if mono { "mono" } else { "" })
                        value=(val)
                        required[f.required]
                        placeholder=(if mono { "(?i)PATTERN" } else { "" });
                }
            }
        }
    }
}

/// Collapsible note field — open when the rule already has a note.
fn render_note(note: &str, readonly: bool) -> Markup {
    if readonly {
        return html! {
            @if !note.is_empty() {
                div.note-block {
                    label { "Note" }
                    span.read-only-val { (note) }
                    input type="hidden" name="note" value=(note);
                }
            } @else {
                input type="hidden" name="note" value="";
            }
        };
    }
    html! {
        details.note-details open[!note.is_empty()] {
            summary.note-toggle { "+ note" }
            div.note-block {
                textarea name="note" rows="2" placeholder="optional note" { (note) }
            }
        }
    }
}

/// The action button row, per mode. Buttons are `type=button` so the
/// click issues the HTMX request rather than a native form submit; being
/// inside `#rule-form` means HTMX serialises the form automatically.
fn render_actions(card: &Card) -> Markup {
    let base = base(card.stage);
    let target = "#detail";

    match card.mode {
        Mode::Edit | Mode::New => {
            let eval_url = match card.id {
                Some(id) => format!("{base}/rule/{id}/evaluate"),
                None => format!("{base}/new/evaluate"),
            };
            html! {
                div.editor-actions {
                    button.btn.btn-shortcut.eval type="button"
                        hx-post=(eval_url) hx-target=(target) hx-swap="innerHTML" hx-indicator="#card-spin"
                    { "[E] Evaluate" }
                    button.btn.btn-shortcut.cancel type="button"
                        hx-get=(base) hx-target=(target) hx-swap="innerHTML"
                    { "[N] Cancel" }
                    @if let Some(id) = card.id {
                        // Delete routes through an impact preview first
                        // (GET), so a rule is never removed un-evaluated.
                        button.btn.btn-shortcut.del type="button" style="margin-left:auto"
                            hx-get=(format!("{base}/rule/{id}/delete")) hx-target=(target) hx-swap="innerHTML"
                        { "\u{1f5d1} Delete" }
                    }
                }
            }
        }
        Mode::Evaluate => {
            let (save_url, back_url) = match card.id {
                Some(id) => (format!("{base}/rule/{id}"), format!("{base}/rule/{id}")),
                None => (format!("{base}/rule"), format!("{base}/new")),
            };
            html! {
                div.editor-actions {
                    @if card.error.is_some() {
                        button.btn.btn-shortcut.save disabled title="fix the pattern to save" { "[Y] Save" }
                    } @else {
                        button.btn.btn-shortcut.save type="button"
                            hx-post=(save_url) hx-target=(target) hx-swap="innerHTML"
                        { "[Y] Save" }
                    }
                    button.btn.btn-shortcut.cancel type="button"
                        hx-get=(back_url) hx-target=(target) hx-swap="innerHTML"
                    { "[B] Back to edit" }
                }
            }
        }
        Mode::EvaluateDelete => {
            let id = card.id.expect("delete targets an existing rule");
            html! {
                div.editor-actions {
                    // Mouse-only confirm (destructive actions are never
                    // bound to a single key, editable-rules-ui §3.4).
                    button.btn.btn-shortcut.del type="button"
                        hx-post=(format!("{base}/rule/{id}/delete")) hx-target=(target) hx-swap="innerHTML"
                    { "\u{1f5d1} Confirm delete" }
                    button.btn.btn-shortcut.cancel type="button"
                        hx-get=(format!("{base}/rule/{id}")) hx-target=(target) hx-swap="innerHTML"
                    { "[B] Back to edit" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merchant() -> RuleData {
        RuleData::Merchant {
            canonical: "Amazon".into(),
            pattern: "(?i)AMAZON".into(),
            note: None,
        }
    }

    fn prefix() -> RuleData {
        RuleData::Prefix {
            pattern: "^POS ".into(),
            gateway: None,
            operation: Some("Purchase".into()),
            has_account: true,
            has_date: false,
            note: Some("a note".into()),
        }
    }

    #[test]
    fn edit_mode_renders_enabled_inputs_and_evaluate_target() {
        let d = merchant();
        let card = Card {
            stage: Stage::Merchants,
            mode: Mode::Edit,
            id: Some(7),
            data: &d,
            error: None,
            eval_body: html! {},
        };
        let h = render(&card).into_string();
        assert!(h.contains("edit mode"), "{h}");
        assert!(h.contains("name=\"canonical\""), "{h}");
        assert!(h.contains("value=\"Amazon\""), "{h}");
        // Evaluate posts the form to the existing-rule evaluate endpoint.
        assert!(h.contains("hx-post=\"/pipeline/stage/merchants/rule/7/evaluate\""), "{h}");
        // Delete routes through the impact preview (GET), with a bin icon,
        // and is NOT a direct POST to /delete from edit mode.
        assert!(h.contains("\u{1f5d1} Delete"), "bin icon + label: {h}");
        assert!(h.contains("hx-get=\"/pipeline/stage/merchants/rule/7/delete\""), "{h}");
        assert!(!h.contains("hx-post=\"/pipeline/stage/merchants/rule/7/delete\""), "edit must not POST-delete: {h}");
        assert!(!h.contains("[Y] Save"), "{h}");
    }

    #[test]
    fn new_mode_has_no_delete_and_posts_to_create() {
        let d = merchant();
        let card = Card {
            stage: Stage::Merchants,
            mode: Mode::New,
            id: None,
            data: &d,
            error: None,
            eval_body: html! {},
        };
        let h = render(&card).into_string();
        assert!(h.contains("new rule"), "{h}");
        assert!(h.contains("hx-post=\"/pipeline/stage/merchants/new/evaluate\""), "{h}");
        assert!(!h.contains("/delete"), "new rule has nothing to delete: {h}");
    }

    #[test]
    fn evaluate_mode_readonly_with_hidden_inputs_and_save() {
        let d = merchant();
        let card = Card {
            stage: Stage::Merchants,
            mode: Mode::Evaluate,
            id: Some(7),
            data: &d,
            error: None,
            eval_body: html! { div.test-marker { "buckets here" } },
        };
        let h = render(&card).into_string();
        assert!(h.contains("evaluate mode"), "{h}");
        // Read-only display + hidden input carrying the value for Save.
        assert!(h.contains("read-only-val"), "{h}");
        assert!(h.contains("type=\"hidden\" name=\"canonical\" value=\"Amazon\""), "{h}");
        // Save posts to the edit endpoint; eval body embedded.
        assert!(h.contains("hx-post=\"/pipeline/stage/merchants/rule/7\""), "{h}");
        assert!(h.contains("buckets here"), "{h}");
        // Delete is NOT offered in plain evaluate mode (edit mode only).
        assert!(!h.contains("Delete"), "evaluate mode must not show Delete: {h}");
    }

    #[test]
    fn evaluate_delete_mode_confirms_via_post_and_is_mouse_only() {
        let d = merchant();
        let card = Card {
            stage: Stage::Merchants,
            mode: Mode::EvaluateDelete,
            id: Some(7),
            data: &d,
            error: None,
            eval_body: html! { div { "delete impact" } },
        };
        let h = render(&card).into_string();
        assert!(h.contains("confirm delete"), "{h}");
        // Confirm delete is the POST; fields are read-only.
        assert!(h.contains("\u{1f5d1} Confirm delete"), "{h}");
        assert!(h.contains("hx-post=\"/pipeline/stage/merchants/rule/7/delete\""), "{h}");
        assert!(h.contains("read-only-val"), "{h}");
        // Mouse-only: the confirm button carries no `.save` class, so the
        // Y keyboard shortcut can't trigger it.
        assert!(!h.contains("btn-shortcut save"), "delete confirm must not be Y-bound: {h}");
    }

    #[test]
    fn evaluate_with_error_disables_save() {
        let d = merchant();
        let card = Card {
            stage: Stage::Merchants,
            mode: Mode::Evaluate,
            id: Some(7),
            data: &d,
            error: Some("syntax error: regex parse error: unbalanced"),
            eval_body: html! {},
        };
        let h = render(&card).into_string();
        assert!(h.contains("syntax error: regex parse error"), "{h}");
        assert!(h.contains("save disabled") || h.contains("disabled"), "Save must be disabled: {h}");
    }

    #[test]
    fn prefix_renders_capture_flags_select_and_open_note() {
        let d = prefix();
        let card = Card {
            stage: Stage::Prefixes,
            mode: Mode::Edit,
            id: Some(3),
            data: &d,
            error: None,
            eval_body: html! {},
        };
        let h = render(&card).into_string();
        // Capture flags rendered as checkboxes; has_account checked.
        assert!(h.contains("name=\"has_account\""), "{h}");
        assert!(h.contains("checked"), "has_account is on: {h}");
        // Operation select carries the saved value as selected.
        assert!(h.contains("<select"), "{h}");
        assert!(h.contains("selected"), "{h}");
        // Note present → details open with the note text.
        assert!(h.contains("a note"), "{h}");
    }
}
