//! Page layout for the `/transactions/*` tab. Renders the
//! reverse-chronological transaction river with three-pillar cleaning
//! state visible on each row. Detail and activity panels are still
//! placeholders; they are filled in by subsequent commits.
//!
//! The tab is, by design, mostly a *view* over data the existing
//! handlers already manage. Mutation goes through the staging
//! endpoints exposed by the `transfers` and `normalise` tabs (plus the
//! new `/transfer-decisions/*` endpoints once they land).

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use crate::helpers::{format_dollars, format_dollars_compact, format_short_date};
use crate::state::AppState;

use super::helpers::TxnQueueRow;
use super::state::{CatState, NormState, PairState};

/// One row of the queue panel, decorated with its three-pillar
/// cleaning state. Bundling the row and its derived states makes the
/// view function pure: it just renders, no DB access.
#[derive(Debug, Clone)]
pub struct QueueRowView {
    pub row: TxnQueueRow,
    pub pair: PairState,
    pub norm: NormState,
    pub cat: CatState,
}

/// Render the full `/transactions/` page. Fetches the most recent
/// transactions, decorates each with its three-pillar cleaning state,
/// and renders the queue panel.
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let st = state.lock().unwrap();
    // 200 rows is enough to fill the panel on first paint without
    // hammering SQLite. Pagination / load-older is a later commit.
    let rows = super::helpers::recent_transactions(&st.conn, 200).unwrap_or_default();
    let views: Vec<QueueRowView> = rows
        .into_iter()
        .map(|r| {
            let pair = super::state::derive_pair_state(&st.conn, r.id, r.is_transfer)
                .unwrap_or(PairState::NotApplicable);
            let norm = super::state::derive_norm_state(&st.conn, r.original_payee.as_deref())
                .unwrap_or(NormState::Missing);
            let cat = super::state::derive_cat_state(r.category_id);
            QueueRowView { row: r, pair, norm, cat }
        })
        .collect();

    let n_views = views.len();
    let queue = render_queue(&views, None);
    let detail = html! {
        div.empty-state { p { "Select a transaction from the queue." } }
    };
    let activity = html! {
        div.activity-header {
            span.stat { (n_views) " transactions visible. Detail and actions land in subsequent commits." }
        }
    };
    crate::render::render_page("transactions", "Transactions", queue, detail, activity)
}

/// Render the queue panel from a slice of pre-decorated rows. Pure
/// function: no DB access, no state lock. The caller is responsible
/// for fetching rows and deriving their three-pillar state (see
/// `helpers::recent_transactions` and `state::derive_*`).
///
/// `active_id` is the currently-selected transaction id, used to add
/// the `.selected` CSS class. `None` means no selection (initial
/// render before the user has navigated).
pub fn render_queue(views: &[QueueRowView], active_id: Option<i64>) -> Markup {
    html! {
        div.queue-header {
            h2 { (views.len()) " transactions" }
        }
        div.queue-list {
            @for v in views {
                (render_queue_row(v, active_id == Some(v.row.id)))
            }
        }
    }
}

/// Render a single queue row. New row shape (round-3 review):
///
/// ```text
///   [norm-glyph] [date] [payee] [pair-glyph?] [cat-tag] [amount]
/// ```
///
/// - **Norm glyph** is always present, leftmost. The pipeline always
///   has *something* to say about every payee (✅ confirmed,
///   🔍 pending review, ❓ no rule, 🚫 rejected).
/// - **Pair glyph** is conditional: rendered only when the pillar has
///   a state worth flagging (paired, pending review, suspected pair).
///   Most rows aren't transfers, so we save horizontal space by
///   omitting the slot entirely for `Rejected` and `NotApplicable`.
/// - **Cat tag** is a pill (`.cat-tag`), not an emoji. Variants:
///   `cat-tag-confirmed` shows the category name; `cat-tag-pending`
///   shows name plus `?`; `cat-tag-missing` shows just `?`;
///   `cat-tag-rejected` shows `×`.
///
/// Every glyph and tag carries a `title="..."` tooltip so the
/// vocabulary is discoverable on hover.
fn render_queue_row(v: &QueueRowView, is_selected: bool) -> Markup {
    let detail_url = format!("/transactions/txn/{}", v.row.id);
    let amount_class = if v.row.amount_cents >= 0 {
        "amount amount-positive"
    } else {
        "amount amount-negative"
    };
    let signed_amount = if v.row.amount_cents >= 0 {
        format!("+{}", format_dollars_compact(v.row.amount_cents))
    } else {
        format!("-{}", format_dollars_compact(v.row.amount_cents))
    };
    html! {
        div.queue-item.(if is_selected { "selected" } else { "" })
            hx-get=(detail_url)
            hx-target="#detail"
            hx-swap="innerHTML"
            data-detail-url=(detail_url)
            data-detail-target="#detail"
        {
            (norm_glyph_with_tooltip(v.norm))
            span.date { (format_short_date(&v.row.date)) }
            span.payee { (v.row.payee) }
            (pair_glyph_optional(v.pair))
            (cat_tag(v.cat, v.row.category_title.as_deref()))
            span.(amount_class) { (signed_amount) }
        }
    }
}

fn norm_glyph_with_tooltip(s: NormState) -> Markup {
    let (cls, title) = match s {
        NormState::Confirmed => ("g-norm-confirmed", "normalisation rule confirmed"),
        NormState::Pending => ("g-norm-pending", "normalisation pending review"),
        NormState::Missing => ("g-norm-missing", "no normalisation rule"),
        NormState::Rejected => ("g-norm-rejected", "normalisation rejected"),
    };
    html! { span.(cls) title=(title) {} }
}

/// Pair glyph that hides itself for `NotApplicable` and `Rejected`
/// (those are the "nothing to act on" states; render a zero-width
/// span so the grid column still aligns but no glyph is visible).
fn pair_glyph_optional(s: PairState) -> Markup {
    let (cls, title) = match s {
        PairState::Confirmed => ("g-pair-confirmed", "transfer pair confirmed"),
        PairState::Pending => ("g-pair-pending", "transfer pair pending review"),
        PairState::Orphan => ("g-pair-orphan", "orphan transfer (looks like a transfer; no pair found)"),
        // Render an empty placeholder span (no class, no glyph) so the
        // grid column collapses to its minimum without leaving stale
        // pair-rejected / pair-none classes in the DOM that the test
        // suite actively checks against.
        PairState::Rejected | PairState::NotApplicable => return html! { span.pair-empty {} },
    };
    html! { span.(cls) title=(title) {} }
}

/// Render the category tag. Pill-shaped span with a per-state class
/// the CSS uses to colour. Content depends on state:
/// - Confirmed: `<title>`
/// - Pending:   `<title> ?`
/// - Missing:   `?`
/// - Rejected:  `×`  (multiplication sign, NOT an emoji — the user
///                     specifically asked for this so it visually
///                     reads as "struck out" rather than another
///                     status emoji)
fn cat_tag(s: CatState, title_opt: Option<&str>) -> Markup {
    match s {
        CatState::Confirmed => {
            let name = title_opt.unwrap_or("?");
            let tt = format!("category: {name}");
            html! { span.cat-tag.cat-tag-confirmed title=(tt) { (name) } }
        }
        CatState::Missing => {
            html! { span.cat-tag.cat-tag-missing title="uncategorised" { "?" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::helpers::TxnQueueRow;

    fn row(id: i64, payee: &str, amount_cents: i64, original: Option<&str>) -> TxnQueueRow {
        TxnQueueRow {
            id,
            date: "2026-04-15".to_string(),
            payee: payee.to_string(),
            amount_cents,
            account_name: Some("Cheque".to_string()),
            original_payee: original.map(str::to_string),
            category_id: None,
            category_title: None,
            is_transfer: false,
        }
    }

    fn view(
        id: i64,
        payee: &str,
        amount_cents: i64,
        pair: PairState,
        norm: NormState,
        cat: CatState,
    ) -> QueueRowView {
        QueueRowView {
            row: row(id, payee, amount_cents, Some(payee)),
            pair,
            norm,
            cat,
        }
    }

    #[test]
    fn render_queue_emits_one_queue_item_per_row() {
        let views = vec![
            view(1, "Woolworths", -2000, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
            view(2, "Starbucks", -550, PairState::NotApplicable, NormState::Missing, CatState::Missing),
            view(3, "Uber", -1850, PairState::NotApplicable, NormState::Pending, CatState::Missing),
        ];
        let html = render_queue(&views, None).into_string();
        let n = html.matches("class=\"queue-item").count();
        assert_eq!(n, 3, "expected 3 queue items, html:\n{}", html);
    }

    #[test]
    fn render_queue_row_layout_norm_left_pair_right_cat_tag() {
        // New row layout:
        //   [norm-glyph] [date] [payee] [pair-glyph?] [cat-tag] [amount]
        // Norm glyph is always present (one of ✅/🔍/❓/🚫).
        // Pair glyph only present when the row is paired/pending/orphan.
        // Cat tag is a span.cat-tag with text content (name, name+?, ?, or x).
        let views = vec![view(
            1,
            "Woolworths",
            -2000,
            PairState::NotApplicable,
            NormState::Confirmed,
            CatState::Confirmed,
        )];
        let html = render_queue(&views, None).into_string();
        // Norm glyph class present.
        assert!(html.contains("g-norm-confirmed"), "norm glyph missing in: {html}");
        // Pair glyph (any) absent for non-transfer row.
        for cls in ["g-pair-confirmed", "g-pair-pending", "g-pair-orphan"] {
            assert!(
                !html.contains(cls),
                "unexpected pair glyph {cls} in non-transfer row: {html}"
            );
        }
        // Category renders as a tag, not as a g-cat-* emoji.
        assert!(
            html.contains("class=\"cat-tag"),
            "expected .cat-tag span in: {html}"
        );
        // The tag should contain the category title we passed (none in
        // the helper here, so we get a sentinel instead). The default
        // helper builds rows without category_title -- a separate test
        // covers tag content with a real title.
    }

    #[test]
    fn render_queue_row_pair_glyph_present_only_when_relevant() {
        // Pair glyph should appear for Confirmed, Pending, Orphan;
        // omitted for Rejected and NotApplicable.
        let cases = [
            (PairState::Confirmed, true, "g-pair-confirmed"),
            (PairState::Pending, true, "g-pair-pending"),
            (PairState::Orphan, true, "g-pair-orphan"),
            (PairState::Rejected, false, "g-pair-rejected"),
            (PairState::NotApplicable, false, "g-none"),
        ];
        for (state, should_show, cls) in cases {
            let views = vec![view(
                1,
                "X",
                -100,
                state,
                NormState::Confirmed,
                CatState::Confirmed,
            )];
            let html = render_queue(&views, None).into_string();
            if should_show {
                assert!(
                    html.contains(cls),
                    "expected {cls} for pair state {state:?} in: {html}"
                );
            } else {
                assert!(
                    !html.contains(cls),
                    "did NOT expect {cls} for pair state {state:?} in: {html}"
                );
            }
        }
    }

    #[test]
    fn render_queue_row_cat_tag_renders_title_or_question_or_cross() {
        // Categorised: tag shows the category title.
        let mut v = view(
            1, "X", -100, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed,
        );
        v.row.category_title = Some("Eating Out".to_string());
        let html = render_queue(&[v], None).into_string();
        assert!(html.contains("Eating Out"), "expected category title in tag: {html}");
        assert!(html.contains("cat-tag-confirmed"), "expected confirmed tag class: {html}");

        // Uncategorised: tag shows just "?".
        let v = view(
            1, "Y", -100, PairState::NotApplicable, NormState::Confirmed, CatState::Missing,
        );
        let html = render_queue(&[v], None).into_string();
        assert!(html.contains("cat-tag-missing"), "expected missing tag class: {html}");
        assert!(html.contains(">?<"), "expected literal '?' as tag content: {html}");
    }

    #[test]
    fn render_queue_row_emits_tooltips_on_glyphs_and_tag() {
        // Tooltips are how the user re-discovers the meaning of each
        // glyph and tag without leaving the page.
        let mut v = view(
            1, "X", -100, PairState::Orphan, NormState::Missing, CatState::Confirmed,
        );
        v.row.category_title = Some("Eating Out".to_string());
        let html = render_queue(&[v], None).into_string();
        // Norm "missing" should have a 'no rule' tooltip.
        assert!(
            html.contains("title=\"no normalisation rule\""),
            "expected norm-missing tooltip: {html}"
        );
        // Pair "orphan" should have a 'looks like a transfer' tooltip.
        assert!(
            html.contains("title=\"orphan transfer"),
            "expected pair-orphan tooltip: {html}"
        );
        // Cat tag should have a 'category' tooltip.
        assert!(
            html.contains("title=\"category: Eating Out\""),
            "expected cat-tag tooltip: {html}"
        );
    }

    #[test]
    fn render_queue_marks_active_row_with_selected_class() {
        let views = vec![
            view(1, "A", -100, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
            view(2, "B", -200, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
            view(3, "C", -300, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
        ];
        let html = render_queue(&views, Some(2)).into_string();
        // exactly one queue-item should carry the .selected class.
        assert_eq!(
            html.matches("queue-item selected").count(),
            1,
            "expected exactly one selected row, html:\n{html}"
        );
    }

    #[test]
    fn render_queue_data_attributes_drive_htmx_swap() {
        // The JS in js.rs reads data-detail-url + data-detail-target on
        // each queue item to drive arrow-key navigation. We must emit
        // both, pointing at the per-row detail fragment endpoint and
        // at #detail.
        let views = vec![view(
            42,
            "Woolworths",
            -2000,
            PairState::NotApplicable,
            NormState::Confirmed,
            CatState::Confirmed,
        )];
        let html = render_queue(&views, None).into_string();
        assert!(
            html.contains("data-detail-url=\"/transactions/txn/42\""),
            "expected per-row detail URL, html:\n{html}"
        );
        assert!(
            html.contains("data-detail-target=\"#detail\""),
            "expected #detail swap target, html:\n{html}"
        );
    }

    #[test]
    fn render_queue_amount_is_signed_dollars_and_coloured() {
        // -2000 cents => -$20.00 and class amount-negative.
        // +1234 cents => +$12.34 and class amount-positive.
        let views = vec![
            view(1, "Outflow", -2000, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
            view(2, "Inflow", 1234, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed),
        ];
        let html = render_queue(&views, None).into_string();
        assert!(html.contains("amount-negative"), "html:\n{html}");
        assert!(html.contains("amount-positive"), "html:\n{html}");
        assert!(html.contains("$20.00"), "html:\n{html}");
        assert!(html.contains("$12.34"), "html:\n{html}");
    }
}

/// Render the per-transaction detail fragment served by
/// `GET /transactions/txn/<id>`. Static for now: just the header,
/// account, amount, and three cleaning-state cards (one per pillar
/// that needs the user's attention). Action buttons in the cards are
/// rendered but not wired up; the action plumbing lands in a later
/// commit.
pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, txn_id: i64) -> Markup {
    let st = state.lock().unwrap();
    let row_opt: Option<TxnQueueRow> = super::helpers::recent_transactions(&st.conn, 100_000)
        .ok()
        .and_then(|rows| rows.into_iter().find(|r| r.id == txn_id));

    let Some(row) = row_opt else {
        return html! {
            div.empty-state { p { "Transaction not found." } }
        };
    };

    let pair = super::state::derive_pair_state(&st.conn, row.id, row.is_transfer)
        .unwrap_or(PairState::NotApplicable);
    let norm = super::state::derive_norm_state(&st.conn, row.original_payee.as_deref())
        .unwrap_or(NormState::Missing);
    let cat = super::state::derive_cat_state(row.category_id);
    render_detail(&row, pair, norm, cat)
}

/// Pure rendering of the detail panel content. Split out so tests can
/// drive it without database setup.
fn render_detail(row: &TxnQueueRow, pair: PairState, norm: NormState, cat: CatState) -> Markup {
    let amount_class = if row.amount_cents >= 0 { "amount-positive" } else { "amount-negative" };
    let signed = if row.amount_cents >= 0 {
        format!("+{}", format_dollars(row.amount_cents))
    } else {
        format!("-{}", format_dollars(row.amount_cents))
    };
    html! {
        div.detail-header {
            div.row {
                h2 {
                    (row.payee)
                    " "
                    span.glyphs {
                        (norm_glyph_with_tooltip(norm))
                        (pair_glyph_optional(pair))
                    }
                    " "
                    (cat_tag(cat, row.category_title.as_deref()))
                }
                span.amount-big.(amount_class) { (signed) }
            }
            div.meta {
                span { (format_short_date(&row.date)) }
                @if let Some(name) = &row.account_name {
                    span { (name) }
                }
                @if row.original_payee.as_deref() != Some(row.payee.as_str())
                    && row.original_payee.is_some()
                {
                    span.chip { "raw: " (row.original_payee.as_deref().unwrap_or("")) }
                }
            }
        }

        (render_pair_card(pair))
        (render_norm_card(norm, row.original_payee.as_deref()))
        (render_cat_card(cat))

        div.note {
            "Action wiring (Y / N / S) lands in a follow-up commit. For now the cards are read-only."
        }
    }
}

fn render_pair_card(s: PairState) -> Markup {
    let (cls, title, sub) = match s {
        PairState::Confirmed => (
            "ok",
            "Pair confirmed",
            "This transaction is paired with its counterpart.",
        ),
        PairState::Pending => (
            "warn",
            "Pair proposed",
            "The pairing pipeline proposed a counterpart \u{2014} your call to confirm.",
        ),
        PairState::Orphan => (
            "bad",
            "Looks like a transfer, no pair found",
            "Either the counterpart hasn't synced yet, this isn't a real internal transfer, or the pairing pipeline missed it.",
        ),
        PairState::Rejected => (
            "ok",
            "Pair rejected",
            "You've decided this is not a transfer.",
        ),
        PairState::NotApplicable => return html! {},
    };
    html! {
        div.cleaning-card.(cls) {
            span.glyph.(pair_glyph_class(s)) { }
            div {
                div.title { (title) }
                div.sub { (sub) }
            }
        }
    }
}

fn render_norm_card(s: NormState, original_payee: Option<&str>) -> Markup {
    let (cls, title, sub) = match s {
        NormState::Confirmed => (
            "ok",
            "Normalisation rule confirmed",
            "Payee has a confirmed normalisation rule.",
        ),
        NormState::Pending => (
            "warn",
            "Normalisation rule pending review",
            "The pipeline produced a proposal \u{2014} your call to confirm.",
        ),
        NormState::Missing => (
            "bad",
            "No normalisation rule",
            "The pipeline has nothing to say about this payee. Either teach it a rule, or ignore.",
        ),
        NormState::Rejected => (
            "ok",
            "Normalisation rule rejected",
            "You've decided this payee should not be normalised.",
        ),
    };
    let _ = original_payee; // for future commits when we link to /normalise/item/<slug>
    html! {
        div.cleaning-card.(cls) {
            span.glyph.(norm_glyph_class(s)) { }
            div {
                div.title { (title) }
                div.sub { (sub) }
            }
        }
    }
}

fn render_cat_card(s: CatState) -> Markup {
    let (cls, title, sub) = match s {
        CatState::Confirmed => (
            "ok",
            "Categorised",
            "This transaction has a category.",
        ),
        CatState::Missing => (
            "bad",
            "Uncategorised",
            "This transaction has no category. Mutation is out of scope for v1 \u{2014} fix it in PocketSmith and re-sync.",
        ),
    };
    html! {
        div.cleaning-card.(cls) {
            span.glyph.(cat_glyph_class(s)) { }
            div {
                div.title { (title) }
                div.sub { (sub) }
            }
        }
    }
}

// Class-only variants of the glyph fns so the detail panel can use
// them inside .cleaning-card .glyph wrappers (rather than a span).
fn pair_glyph_class(s: PairState) -> &'static str {
    match s {
        PairState::Confirmed => "g-pair-confirmed",
        PairState::Pending => "g-pair-pending",
        PairState::Orphan => "g-pair-orphan",
        PairState::Rejected => "g-pair-rejected",
        PairState::NotApplicable => "g-none",
    }
}
fn norm_glyph_class(s: NormState) -> &'static str {
    match s {
        NormState::Confirmed => "g-norm-confirmed",
        NormState::Pending => "g-norm-pending",
        NormState::Missing => "g-norm-missing",
        NormState::Rejected => "g-norm-rejected",
    }
}
fn cat_glyph_class(s: CatState) -> &'static str {
    match s {
        CatState::Confirmed => "g-cat-confirmed",
        CatState::Missing => "g-cat-missing",
    }
}

#[cfg(test)]
mod detail_tests {
    use super::*;
    use crate::transactions::helpers::TxnQueueRow;

    fn row(amount_cents: i64) -> TxnQueueRow {
        TxnQueueRow {
            id: 1,
            date: "2026-04-15".to_string(),
            payee: "Amazon Marketplace".to_string(),
            amount_cents,
            account_name: Some("Amex Platinum".to_string()),
            original_payee: Some("AMAZON MARKETPLACE".to_string()),
            category_id: None,
            category_title: None,
            is_transfer: false,
        }
    }

    #[test]
    fn render_detail_emits_three_cleaning_cards_for_a_messy_row() {
        let html = render_detail(
            &row(-1699),
            PairState::Orphan,
            NormState::Missing,
            CatState::Missing,
        )
        .into_string();
        // Three distinct cards, one per pillar.
        assert_eq!(
            html.matches("class=\"cleaning-card").count(),
            3,
            "expected 3 cleaning cards, html:\n{html}"
        );
        // Each pillar's "needs you" glyph class is present.
        for cls in ["g-pair-orphan", "g-norm-missing", "g-cat-missing"] {
            assert!(
                html.contains(cls),
                "expected {cls} in detail html"
            );
        }
    }

    #[test]
    fn render_detail_omits_pair_card_when_not_applicable() {
        // A non-transfer row with confirmed norm + cat: only the
        // norm+cat cards should render, both with .ok class. No pair
        // card at all (PairState::NotApplicable returns empty markup).
        let html = render_detail(
            &row(-1234),
            PairState::NotApplicable,
            NormState::Confirmed,
            CatState::Confirmed,
        )
        .into_string();
        assert_eq!(
            html.matches("class=\"cleaning-card").count(),
            2,
            "expected 2 cleaning cards (no pair card), html:\n{html}"
        );
        assert!(
            !html.contains("g-pair-"),
            "no pair-* class expected for not-applicable row, html:\n{html}"
        );
    }

    #[test]
    fn render_detail_shows_amount_signed_and_coloured() {
        let html = render_detail(
            &row(-1699),
            PairState::NotApplicable,
            NormState::Missing,
            CatState::Missing,
        )
        .into_string();
        assert!(html.contains("amount-negative"), "html:\n{html}");
        assert!(html.contains("$16.99"), "html:\n{html}");

        let html = render_detail(
            &row(2500),
            PairState::NotApplicable,
            NormState::Missing,
            CatState::Missing,
        )
        .into_string();
        assert!(html.contains("amount-positive"), "html:\n{html}");
        assert!(html.contains("$25.00"), "html:\n{html}");
    }

    #[test]
    fn render_detail_shows_raw_payee_chip_when_normalised_differs() {
        // payee "Amazon Marketplace" vs original "AMAZON MARKETPLACE"
        // differs => raw chip should render.
        let html = render_detail(
            &row(-1699),
            PairState::NotApplicable,
            NormState::Confirmed,
            CatState::Confirmed,
        )
        .into_string();
        assert!(
            html.contains("AMAZON MARKETPLACE"),
            "expected raw original_payee chip, html:\n{html}"
        );
    }
}
