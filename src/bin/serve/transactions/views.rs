//! Page layout for the `/transactions/*` tab. Renders the
//! reverse-chronological transaction river with three-pillar cleaning
//! state visible on each row. Detail and activity panels are still
//! placeholders; they are filled in by subsequent commits.
//!
//! The tab is, by design, mostly a *view* over data the existing
//! handlers already manage. Mutation goes through the staging
//! endpoints exposed by the `transfers` and `normalise` tabs (plus the
//! new `/transfer-decisions/*` endpoints once they land).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::normalise::{normalise as run_normalise, NormalisationResult, TraceEntry};

use crate::helpers::{format_dollars, format_dollars_compact, format_short_date};
use crate::state::{AppState, Decision};

use super::helpers::{TxnFilter, TxnQueueRow};
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

/// Build a memo from `original_payee` to "does the pipeline trace
/// transform this payee at all" for the rows that need it. Only rows
/// with no staging row (`norm_status` is `None`) are computed, since
/// that's the only case where trace-emptiness changes the derived
/// [`NormState`] (`Clean` vs `Missing`). Running the pipeline once per
/// distinct payee keeps this ~tens of ms even on a full 1000-row
/// queue.
fn build_trace_memo(rows: &[TxnQueueRow]) -> HashMap<String, bool> {
    let mut memo: HashMap<String, bool> = HashMap::new();
    for r in rows {
        if r.norm_status.is_some() {
            continue;
        }
        if let Some(op) = r.original_payee.as_deref() {
            memo.entry(op.to_string())
                .or_insert_with(|| !run_normalise(op).trace.is_empty());
        }
    }
    memo
}

/// Look up the memoised trace-emptiness for a row. Rows that carry a
/// staging row (`norm_status.is_some()`) never need this, so a missing
/// memo entry safely reads as `false`.
fn trace_nonempty(memo: &HashMap<String, bool>, r: &TxnQueueRow) -> bool {
    r.original_payee
        .as_deref()
        .and_then(|op| memo.get(op))
        .copied()
        .unwrap_or(false)
}

/// Render the full `/transactions/` page. Fetches transactions
/// matching the active filter, decorates each with its three-pillar
/// cleaning state, and renders the queue panel. The detail panel
/// renders the currently-active row (if any); the activity panel
/// shows the session's recent decisions with undo buttons.
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let st = state.lock().unwrap();
    let filter = TxnFilter::parse(&st.txn_filter);
    let rows = super::helpers::filtered_transactions(&st.conn, filter, 1000).unwrap_or_default();
    // No per-row SQL needed -- pair_status and norm_status arrive
    // pre-fetched via LEFT JOIN in filtered_transactions.
    let trace_memo = build_trace_memo(&rows);
    let views: Vec<QueueRowView> = rows
        .into_iter()
        .map(|r| {
            let pair = super::state::pair_state_from_status(r.pair_status, r.is_transfer);
            let norm = super::state::norm_state_from_status(r.norm_status, trace_nonempty(&trace_memo, &r));
            let cat = super::state::derive_cat_state(r.category_id);
            QueueRowView { row: r, pair, norm, cat }
        })
        .collect();

    let active_id = st.txn_active;
    let queue = render_queue_with_header(&views, active_id, filter);
    let detail = render_active_detail(&st, &views, active_id);
    let activity = render_activity(&st);
    let chips = crate::freshness::header_chips(&st.conn);
    crate::render::render_page_with_chips(
        "transactions",
        "Transactions",
        chips,
        queue,
        detail,
        activity,
    )
}

/// Render the detail panel for the active row, or an empty-state
/// placeholder if no row is selected. We re-use the QueueRowView
/// already computed for the queue (avoids a second round-trip per
/// page render).
fn render_active_detail(
    state: &AppState,
    views: &[QueueRowView],
    active_id: Option<i64>,
) -> Markup {
    let Some(id) = active_id else {
        return html! { div.empty-state { p { "Select a transaction from the queue." } } };
    };
    // Cheap path: the active row is in the rendered queue subset.
    if let Some(v) = views.iter().find(|x| x.row.id == id) {
        let pipeline = v.row.original_payee.as_deref().map(run_normalise);
        let siblings = v
            .row
            .original_payee
            .as_deref()
            .map(|op| crate::normalise::helpers::matching_transactions(&state.conn, op))
            .unwrap_or_default();
        return render_detail(&v.row, v.pair, v.norm, v.cat, pipeline.as_ref(), &siblings);
    }
    // Fallback: the active row is no longer in the filtered view
    // (e.g. an action just resolved it and the resolver decided to
    // stay anchored on it -- see handlers::pick_next_active). Fetch
    // the row by id directly so the detail panel still shows what
    // the user last acted on, with its updated state.
    let Some(row) = super::helpers::fetch_by_id(&state.conn, id).unwrap_or(None) else {
        return html! { div.empty-state { p { "Transaction not found." } } };
    };
    let pair = super::state::derive_pair_state(&state.conn, row.id, row.is_transfer)
        .unwrap_or(PairState::NotApplicable);
    let pipeline = row.original_payee.as_deref().map(run_normalise);
    let trace_nonempty = pipeline.as_ref().map(|p| !p.trace.is_empty()).unwrap_or(false);
    let norm = super::state::derive_norm_state(&state.conn, row.original_payee.as_deref(), trace_nonempty)
        .unwrap_or(NormState::Missing);
    let cat = super::state::derive_cat_state(row.category_id);
    let siblings = row
        .original_payee
        .as_deref()
        .map(|op| crate::normalise::helpers::matching_transactions(&state.conn, op))
        .unwrap_or_default();
    render_detail(&row, pair, norm, cat, pipeline.as_ref(), &siblings)
}

/// Render the activity panel for the Transactions tab. Mirrors the
/// layout used by the Normalise tab: top row of session counters,
/// list of recent activity entries each with an undo-btn (the JS
/// keyboard handler binds `U` to the first .undo-btn it finds).
fn render_activity(state: &AppState) -> Markup {
    let n_confirm = count_decisions(&state.txn_activity, Decision::Confirm);
    let n_reject = count_decisions(&state.txn_activity, Decision::Reject);
    let n_skip = count_decisions(&state.txn_activity, Decision::Skip);
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (n_confirm) } }
            span.stat { "Rejected " span.count-rejected { (n_reject) } }
            span.stat { "Skipped " span.count-skipped { (n_skip) } }
            span.stat { "Undone " span.count-undone { (state.txn_undone) } }
        }
        div.activity-list {
            @for entry in state.txn_activity.iter().rev().take(20) {
                div.activity-row {
                    span.((match entry.decision {
                        Decision::Confirm => "status-confirmed",
                        Decision::Reject => "status-rejected",
                        Decision::Skip => "status-skipped",
                    })) {
                        @match entry.decision {
                            Decision::Confirm => { "\u{2713} confirmed" },
                            Decision::Reject => { "\u{2717} rejected" },
                            Decision::Skip => { "\u{2298} skipped" },
                        }
                    }
                    span { (entry.payee) }
                    span {
                        @if entry.amount_cents >= 0 { "+" } @else { "-" }
                        (format_dollars_compact(entry.amount_cents))
                    }
                    button.undo-btn
                        hx-post=(format!("/transactions/txn/{}/{}/undo", entry.txn_id, entry.pillar.as_str()))
                        hx-target="body"
                    { "undo" }
                }
            }
        }
    }
}

fn count_decisions(activity: &[crate::state::TxnActivityEntry], d: Decision) -> usize {
    activity.iter().filter(|e| e.decision == d).count()
}

/// Render the queue panel for an HTMX swap. The route handler updates
/// `state.txn_filter` to the requested slug and then asks for this
/// fragment, scoped to `#queue`.
pub fn render_queue_fragment(state: &Arc<Mutex<AppState>>, filter_str: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.txn_filter = filter_str.to_string();
    let filter = TxnFilter::parse(filter_str);
    let rows = super::helpers::filtered_transactions(&st.conn, filter, 1000).unwrap_or_default();
    let trace_memo = build_trace_memo(&rows);
    let views: Vec<QueueRowView> = rows
        .into_iter()
        .map(|r| {
            let pair = super::state::pair_state_from_status(r.pair_status, r.is_transfer);
            let norm = super::state::norm_state_from_status(r.norm_status, trace_nonempty(&trace_memo, &r));
            let cat = super::state::derive_cat_state(r.category_id);
            QueueRowView { row: r, pair, norm, cat }
        })
        .collect();
    render_queue_with_header(&views, st.txn_active, filter)
}

/// Inner queue render with header (count + filter chips). Pure.
fn render_queue_with_header(
    views: &[QueueRowView],
    active_id: Option<i64>,
    filter: TxnFilter,
) -> Markup {
    html! {
        div.queue-header {
            h2 { (views.len()) " transactions" }
            div.filter-row {
                @for f in TxnFilter::ALL {
                    button.filter-btn
                        .(if f == filter { "active" } else { "" })
                        hx-get=(format!("/transactions/queue?filter={}", f.as_str()))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.label()) }
                }
            }
        }
        div.queue-list {
            @for v in views {
                (render_queue_row(v, active_id == Some(v.row.id)))
            }
        }
    }
}

/// Render the queue panel from a slice of pre-decorated rows. Pure
/// function: no DB access, no state lock. The caller is responsible
/// for fetching rows and deriving their three-pillar state (see
/// `helpers::recent_transactions` and `state::derive_*`).
///
/// `active_id` is the currently-selected transaction id, used to add
/// the `.selected` CSS class. `None` means no selection (initial
/// render before the user has navigated).
/// Test-only thin wrapper around the row-rendering loop. Production
/// code goes through `render_queue_with_header`, which adds the count
/// header and the filter chips. The tests focus on per-row markup
/// (emoji classes, data attributes, amount formatting) so they call
/// this header-less variant for cleaner assertions.
#[cfg(test)]
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
            span.date { (format_short_date(&v.row.date)) }
            (norm_glyph_with_tooltip_clickable(v.norm, v.row.id))
            span.payee { (v.row.payee) }
            span.post-payee {
                (pair_glyph_optional_clickable(v.pair, v.row.id))
                (cat_tag_optional(v.cat, v.row.category_title.as_deref()))
            }
            span.(amount_class) { (signed_amount) }
        }
    }
}

fn norm_glyph_with_tooltip(s: NormState) -> Markup {
    norm_glyph_with_tooltip_clickable(s, 0)
}

/// Norm glyph with hover tooltip, optionally clickable when the state
/// represents a decision the user can undo (Confirmed or Rejected).
/// Pass `txn_id = 0` for non-clickable contexts (e.g. detail header).
fn norm_glyph_with_tooltip_clickable(s: NormState, txn_id: i64) -> Markup {
    let (cls, title) = match s {
        NormState::Confirmed => ("g-norm-confirmed", "normalisation rule confirmed"),
        NormState::Pending => ("g-norm-pending", "normalisation pending review"),
        NormState::Clean => ("g-norm-clean", "payee normalised \u{2014} no rule pending"),
        NormState::Missing => ("g-norm-missing", "no normalisation rule"),
        NormState::Rejected => ("g-norm-rejected", "normalisation rejected"),
    };
    let undoable = matches!(s, NormState::Confirmed | NormState::Rejected) && txn_id != 0;
    if undoable {
        let undo_url = format!("/transactions/txn/{txn_id}/norm/undo");
        let title2 = format!("{title} \u{2014} click to undo");
        html! {
            span.(cls).clickable
                title=(title2)
                hx-post=(undo_url)
                hx-target="body"
                onclick="event.stopPropagation()"
            {}
        }
    } else {
        html! { span.(cls) title=(title) {} }
    }
}

fn pair_glyph_optional(s: PairState) -> Markup {
    pair_glyph_optional_clickable(s, 0)
}

fn pair_glyph_optional_clickable(s: PairState, txn_id: i64) -> Markup {
    let (cls, title) = match s {
        PairState::Confirmed => ("g-pair-confirmed", "transfer pair confirmed"),
        PairState::Pending => ("g-pair-pending", "transfer pair pending review"),
        PairState::Orphan => ("g-pair-orphan", "orphan transfer (looks like a transfer; no pair found)"),
        PairState::Rejected | PairState::NotApplicable => return html! {},
    };
    let undoable = matches!(s, PairState::Confirmed) && txn_id != 0;
    if undoable {
        let undo_url = format!("/transactions/txn/{txn_id}/pair/undo");
        let title2 = format!("{title} \u{2014} click to undo");
        html! {
            span.(cls).clickable
                title=(title2)
                hx-post=(undo_url)
                hx-target="body"
                onclick="event.stopPropagation()"
            {}
        }
    } else {
        html! { span.(cls) title=(title) {} }
    }
}

/// Render the category tag, or nothing for `CatState::Missing`
/// (round-4: uncategorised tag was too noisy and most rows are
/// uncategorised). For non-Missing states the tag is a small
/// squared box (`.cat-tag`) carrying the category title.
fn cat_tag_optional(s: CatState, title_opt: Option<&str>) -> Markup {
    match s {
        CatState::Confirmed => {
            let name = title_opt.unwrap_or("—");
            let tt = format!("category: {name}");
            html! { span.cat-tag.cat-tag-confirmed title=(tt) { (name) } }
        }
        CatState::Missing => html! {},
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
            pair_status: None,
            norm_status: None,
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
        // Round-4 row layout:
        //   [date] [norm-glyph] [payee] [post-payee: pair?+cat?] [amount]
        // Date is leftmost (per round-4 feedback).
        // Norm glyph is always present.
        // post-payee groups pair-glyph + cat-tag in a single grid cell
        // so we don't pay two grid gaps when both are absent.
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
        // post-payee container exists.
        assert!(html.contains("class=\"post-payee"), "post-payee span missing: {html}");
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
    fn render_queue_row_cat_tag_renders_title_only_when_categorised() {
        // Categorised: tag shows the category title.
        let mut v = view(
            1, "X", -100, PairState::NotApplicable, NormState::Confirmed, CatState::Confirmed,
        );
        v.row.category_title = Some("Eating Out".to_string());
        let html = render_queue(&[v], None).into_string();
        assert!(html.contains("Eating Out"), "expected category title in tag: {html}");
        assert!(html.contains("cat-tag-confirmed"), "expected confirmed tag class: {html}");

        // Uncategorised: no cat-tag rendered at all (round-4: too noisy).
        let v = view(
            1, "Y", -100, PairState::NotApplicable, NormState::Confirmed, CatState::Missing,
        );
        let html = render_queue(&[v], None).into_string();
        assert!(
            !html.contains("cat-tag"),
            "missing-category row should not render any cat-tag, html:\n{html}"
        );
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
    // Set txn_active so a subsequent body-target re-render keeps this
    // row's detail visible (see render_active_detail in render_page_shell).
    let mut st = state.lock().unwrap();
    st.txn_active = Some(txn_id);
    let row_opt: Option<TxnQueueRow> = super::helpers::fetch_by_id(&st.conn, txn_id).unwrap_or(None);

    let Some(row) = row_opt else {
        return html! {
            div.empty-state { p { "Transaction not found." } }
        };
    };

    let pair = super::state::derive_pair_state(&st.conn, row.id, row.is_transfer)
        .unwrap_or(PairState::NotApplicable);
    let pipeline = row.original_payee.as_deref().map(run_normalise);
    let trace_nonempty = pipeline.as_ref().map(|p| !p.trace.is_empty()).unwrap_or(false);
    let norm = super::state::derive_norm_state(&st.conn, row.original_payee.as_deref(), trace_nonempty)
        .unwrap_or(NormState::Missing);
    let cat = super::state::derive_cat_state(row.category_id);
    let siblings = row
        .original_payee
        .as_deref()
        .map(|op| crate::normalise::helpers::matching_transactions(&st.conn, op))
        .unwrap_or_default();
    render_detail(&row, pair, norm, cat, pipeline.as_ref(), &siblings)
}

/// Pure rendering of the detail panel content. Split out so tests can
/// drive it without database setup.
fn render_detail(
    row: &TxnQueueRow,
    pair: PairState,
    norm: NormState,
    cat: CatState,
    pipeline: Option<&NormalisationResult>,
    siblings: &[crate::normalise::helpers::MatchingTxn],
) -> Markup {
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
                    (cat_tag_optional(cat, row.category_title.as_deref()))
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

        (render_cards_row(pair, norm, cat, row.id))

        @if let Some(p) = pipeline { (render_pipeline_trace(p)) }

        @if !siblings.is_empty() { (render_siblings(row.original_payee.as_deref(), siblings)) }

        div.note {
            "Y / N / S act on whichever pillar is currently up for review (norm-pending or pair-pending). Press the buttons or the corresponding key."
        }
    }
}

/// Render the per-stage transformation trace for the active row.
/// Mirrors the Normalise tab's pipeline-trace section but reuses the
/// same data (NormalisationResult). One row per pipeline stage that
/// mutated the normalised string or attached a feature.
fn render_pipeline_trace(p: &NormalisationResult) -> Markup {
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
    let values: std::collections::HashMap<&str, &str> = entry
        .feature_values
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    html! {
        div.norm-trace-row {
            span.norm-trace-stage { (entry.stage) }
            div.norm-trace-body {
                @if changed_string {
                    div.norm-trace-diff {
                        span.norm-trace-before { (entry.before) }
                        span.norm-trace-arrow { " \u{2192} " }
                        span.norm-trace-after { (entry.after) }
                    }
                }
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
                @if let Some(pat) = entry.matched_pattern {
                    div.norm-trace-pattern {
                        span.norm-trace-pattern-label { "matched" }
                        " "
                        code.norm-trace-pattern-src { (pat) }
                    }
                }
            }
        }
    }
}

/// Sibling transactions sharing the same original_payee. Useful for
/// 'I'm about to confirm this rule \u{2014} how many other txns will it
/// affect, and what do they look like?'
fn render_siblings(
    original_payee: Option<&str>,
    siblings: &[crate::normalise::helpers::MatchingTxn],
) -> Markup {
    let label = original_payee
        .map(|op| format!("sharing original_payee = {op:?}"))
        .unwrap_or_else(|| "siblings".to_string());
    html! {
        div.prior-section {
            h3 { (siblings.len()) " sibling transactions " (label) }
            div.prior-list.norm-txn-list {
                @for t in siblings.iter().take(20) {
                    div.prior-row {
                        span { (format_short_date(&t.date)) }
                        span { (t.payee.as_deref().unwrap_or("\u{2014}")) }
                        span.((if t.amount_cents >= 0 { "amount-positive" } else { "amount-negative" })) {
                            (format_dollars(t.amount_cents))
                        }
                        span.norm-txn-acct { (t.account_name.as_deref().unwrap_or("?")) }
                    }
                }
                @if siblings.len() > 20 {
                    div.prior-row.empty-state-row { "\u{2026} " (siblings.len() - 20) " more not shown" }
                }
            }
        }
    }
}

fn render_action_buttons(action_base: &str) -> Markup {
    // Mirrors the existing render::render_actions but scoped here so
    // we can render the action group inside whatever card needs it,
    // not just at the bottom of the panel. The data-action-base on
    // the .actions div is what js.rs reads for keyboard Y/N/S.
    html! {
        div.actions data-action-base=(action_base) {
            button.btn.btn-confirm
                hx-post=(format!("{action_base}/confirm"))
                hx-target="body"
            { "[Y] Confirm" }
            button.btn.btn-reject
                hx-post=(format!("{action_base}/reject"))
                hx-target="body"
            { "[N] Reject" }
            button.btn.btn-skip
                hx-post=(format!("{action_base}/skip"))
                hx-target="body"
            { "[S] Skip" }
        }
    }
}

/// Render the three cleaning-state cards side-by-side in a horizontal
/// row. Each card uses the same dark-box visual language as the
/// transfers tab's `.txn-card` (no coloured left border), with a
/// status chip in the header and the Y/N/S action buttons rendered
/// inside the card when there's a decision pending. Cards that don't
/// apply (e.g. pair on a non-transfer row) render as a dimmed
/// placeholder so the three-column grid stays balanced.
fn render_cards_row(pair: PairState, norm: NormState, cat: CatState, txn_id: i64) -> Markup {
    html! {
        div.cleaning-cards {
            (render_pair_card(pair, txn_id))
            (render_norm_card(norm, txn_id))
            (render_cat_card(cat))
        }
    }
}

/// Shared status-chip vocabulary used by all three cards in their
/// header strip. Same colour palette as the existing `.btn-*` classes
/// so the user reads green/yellow/red consistently across the page.
#[derive(Copy, Clone)]
enum CardStatus { Ok, Warn, Bad }

impl CardStatus {
    fn class(self) -> &'static str {
        match self {
            CardStatus::Ok => "card-status-ok",
            CardStatus::Warn => "card-status-warn",
            CardStatus::Bad => "card-status-bad",
        }
    }
    fn label(self) -> &'static str {
        match self {
            CardStatus::Ok => "ok",
            CardStatus::Warn => "pending",
            CardStatus::Bad => "needs you",
        }
    }
}

/// Render one cleaning-state card. Layout mirrors the transfers tab's
/// `.txn-card`: header strip with pillar name + status chip, body with
/// glyph + title + sub, optional action buttons at the bottom when the
/// pillar has a decision pending.
fn render_card(
    pillar: &str,
    status: CardStatus,
    glyph_cls: &str,
    title: &str,
    sub: &str,
    action_base: Option<&str>,
) -> Markup {
    html! {
        div.cleaning-card {
            div.cleaning-card-header {
                span.cleaning-card-pillar { (pillar) }
                span.cleaning-card-status.(status.class()) { (status.label()) }
            }
            div.cleaning-card-body {
                span.glyph.(glyph_cls) {}
                div.cleaning-card-text {
                    div.title { (title) }
                    div.sub { (sub) }
                }
            }
            @if let Some(base) = action_base { (render_action_buttons(base)) }
        }
    }
}

/// A dimmed placeholder card used when a pillar doesn't apply to the
/// active row (e.g. Pair on a non-transfer). Keeps the three-column
/// grid balanced so the eye doesn't have to retarget on every row.
fn render_card_placeholder(pillar: &str, reason: &str) -> Markup {
    html! {
        div.cleaning-card.cleaning-card-na {
            div.cleaning-card-header {
                span.cleaning-card-pillar { (pillar) }
                span.cleaning-card-status { "n/a" }
            }
            div.cleaning-card-body { div.cleaning-card-text { div.sub { (reason) } } }
        }
    }
}

fn render_pair_card(s: PairState, txn_id: i64) -> Markup {
    let (status, title, sub, show_actions) = match s {
        PairState::Confirmed => (
            CardStatus::Ok,
            "Pair confirmed",
            "This transaction is paired with its counterpart.",
            false,
        ),
        PairState::Pending => (
            CardStatus::Warn,
            "Pair proposed",
            "The pairing pipeline proposed a counterpart \u{2014} your call to confirm.",
            true,
        ),
        PairState::Orphan => (
            CardStatus::Bad,
            "Looks like a transfer, no pair found",
            "Either the counterpart hasn't synced yet, this isn't a real internal transfer, or the pairing pipeline missed it.",
            false, // orphan-flow needs transfer_decisions (PLAN §8); follow-up commit
        ),
        PairState::Rejected => (
            CardStatus::Ok,
            "Pair rejected",
            "You've decided this is not a transfer.",
            false,
        ),
        PairState::NotApplicable => return render_card_placeholder("Pair", "not a transfer"),
    };
    let action_base = format!("/transactions/txn/{txn_id}/pair");
    render_card("Pair", status, pair_glyph_class(s), title, sub, show_actions.then_some(action_base.as_str()))
}

fn render_norm_card(s: NormState, txn_id: i64) -> Markup {
    let (status, title, sub) = match s {
        NormState::Confirmed => (
            CardStatus::Ok,
            "Normalisation rule confirmed",
            "Payee has a confirmed normalisation rule.",
        ),
        NormState::Pending => (
            CardStatus::Warn,
            "Normalisation rule pending review",
            "The pipeline produced a proposal \u{2014} your call to confirm.",
        ),
        NormState::Clean => (
            CardStatus::Ok,
            "Payee normalised",
            "The pipeline normalises this payee and nothing is pending review.",
        ),
        NormState::Missing => (
            CardStatus::Bad,
            "No normalisation rule",
            "The pipeline has nothing to say about this payee. Either teach it a rule, or ignore.",
        ),
        NormState::Rejected => (
            CardStatus::Ok,
            "Normalisation rule rejected",
            "You've decided this payee should not be normalised.",
        ),
    };
    let action_base = format!("/transactions/txn/{txn_id}/norm");
    let show_actions = matches!(s, NormState::Pending);
    render_card(
        "Normalise", status, norm_glyph_class(s), title, sub,
        show_actions.then_some(action_base.as_str()),
    )
}

fn render_cat_card(s: CatState) -> Markup {
    let (status, title, sub) = match s {
        CatState::Confirmed => (
            CardStatus::Ok,
            "Categorised",
            "This transaction has a category.",
        ),
        CatState::Missing => (
            CardStatus::Bad,
            "Uncategorised",
            "This transaction has no category. Mutation is out of scope for v1 \u{2014} fix it in PocketSmith and re-sync.",
        ),
    };
    render_card("Categorise", status, cat_glyph_class(s), title, sub, None)
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
        NormState::Clean => "g-norm-clean",
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
            pair_status: None,
            norm_status: None,
        }
    }

    #[test]
    fn render_detail_emits_three_cleaning_cards_for_a_messy_row() {
        let html = render_detail(
            &row(-1699),
            PairState::Orphan,
            NormState::Missing,
            CatState::Missing, None, &[])
        .into_string();
        // Three distinct cards, one per pillar.
        assert_eq!(
            html.matches("<div class=\"cleaning-card\">").count()
                + html.matches("<div class=\"cleaning-card cleaning-card-na\">").count(),
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
    fn render_detail_renders_three_cards_horizontally_even_when_pair_na() {
        // New layout (round-5): three cards always rendered in a
        // horizontal row. A non-applicable pair becomes a dimmed
        // placeholder card instead of being omitted, so the grid
        // stays balanced. The pair-glyph classes are still absent
        // because the placeholder has no glyph.
        let html = render_detail(
            &row(-1234),
            PairState::NotApplicable,
            NormState::Confirmed,
            CatState::Confirmed, None, &[])
        .into_string();
        assert_eq!(
            html.matches("<div class=\"cleaning-card\">").count()
                + html.matches("<div class=\"cleaning-card cleaning-card-na\">").count(),
            3,
            "expected 3 cleaning cards (one is the n/a placeholder), html:\n{html}"
        );
        assert!(
            html.contains("cleaning-card-na"),
            "expected the not-applicable placeholder card, html:\n{html}"
        );
        assert!(
            !html.contains("g-pair-"),
            "no pair-* glyph expected for not-applicable row, html:\n{html}"
        );
    }

    #[test]
    fn render_detail_shows_amount_signed_and_coloured() {
        let html = render_detail(
            &row(-1699),
            PairState::NotApplicable,
            NormState::Missing,
            CatState::Missing, None, &[])
        .into_string();
        assert!(html.contains("amount-negative"), "html:\n{html}");
        assert!(html.contains("$16.99"), "html:\n{html}");

        let html = render_detail(
            &row(2500),
            PairState::NotApplicable,
            NormState::Missing,
            CatState::Missing, None, &[])
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
            CatState::Confirmed, None, &[])
        .into_string();
        assert!(
            html.contains("AMAZON MARKETPLACE"),
            "expected raw original_payee chip, html:\n{html}"
        );
    }

    #[test]
    fn norm_card_clean_does_not_flag_no_normalisation() {
        // PR 0: a payee whose pipeline matched (non-empty trace) but has
        // no staging row is Clean, not Missing. Its pillar must NOT read
        // "No normalisation rule".
        let html = render_norm_card(NormState::Clean, 0).into_string();
        assert!(
            !html.contains("No normalisation rule"),
            "Clean state must not flag 'No normalisation rule', html:\n{html}"
        );
        assert!(
            html.contains("Payee normalised"),
            "Clean state should read 'Payee normalised', html:\n{html}"
        );
        // Clean is a benign (ok) pillar, not the red 'bad' variant.
        assert!(html.contains("card-status-ok"), "Clean pillar should be 'ok', html:\n{html}");
    }

    #[test]
    fn norm_card_missing_still_flags_no_normalisation() {
        // Only a completely empty trace (Missing) keeps the old text.
        let html = render_norm_card(NormState::Missing, 0).into_string();
        assert!(
            html.contains("No normalisation rule"),
            "Missing state should still flag 'No normalisation rule', html:\n{html}"
        );
    }

    #[test]
    fn norm_glyph_clean_has_distinct_class_and_tooltip() {
        let html = norm_glyph_with_tooltip(NormState::Clean).into_string();
        assert!(html.contains("g-norm-clean"), "html:\n{html}");
        assert!(!html.contains("no normalisation rule"), "html:\n{html}");
    }
}
