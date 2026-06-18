//! Page layout for the `/categorise/*` tab. Three-pane shell (queue +
//! detail + activity) mirroring the normalise tab, rendering
//! `category_proposals` rows. The hardcoded taxonomy is not user-editable,
//! so this tab is a *proposal review queue*, not a rule editor.
//!
//! `merchant_key`s contain spaces, so they are hex-encoded into URL
//! segments via [`encode_key`] / [`decode_key`].

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::category_proposals::CategoryProposalRow;
use pocketsmith_sync::review::Status;

use crate::state::{AppState, Decision};
use crate::tab::count_decisions;

use super::helpers::{category_title, get_filtered_proposals, CatStatusFilter};

/// Hex-encode a merchant key for use as a URL path segment.
pub fn encode_key(key: &str) -> String {
    key.bytes().map(|b| format!("{b:02x}")).collect()
}

/// Decode a hex URL segment back to the merchant key. Returns `None` on
/// malformed input.
pub fn decode_key(hex: &str) -> Option<String> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    bytes.and_then(|b| String::from_utf8(b).ok())
}

pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut st = state.lock().unwrap();
    let status = CatStatusFilter::parse(&st.cat_status_filter);
    let rows = get_filtered_proposals(&st.conn, status, &st.categorise.decisions);

    let active = st
        .categorise
        .active
        .clone()
        .filter(|k| rows.iter().any(|r| &r.merchant_key == k))
        .or_else(|| rows.first().map(|r| r.merchant_key.clone()));
    st.categorise.active = active.clone();

    render_full_page(&st, &rows, status, active.as_deref())
}

pub fn render_queue_fragment(state: &Arc<Mutex<AppState>>, status_str: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.cat_status_filter = status_str.to_string();
    let status = CatStatusFilter::parse(status_str);
    let rows = get_filtered_proposals(&st.conn, status, &st.categorise.decisions);
    let active = st.categorise.active.clone();
    render_queue(&st, &rows, active.as_deref(), status)
}

pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, key: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.categorise.active = Some(key.to_string());
    match pocketsmith_sync::db::category_proposals::get(&st.conn, key) {
        Ok(Some(row)) => {
            let is_skipped = st.categorise.decisions.get(key) == Some(&Decision::Skip);
            let title = category_title(&st.conn, row.proposed_category);
            render_detail(&row, title.as_deref(), is_skipped)
        }
        _ => html! { div.empty-state { p { "Proposal not found." } } },
    }
}

fn render_full_page(
    state: &AppState,
    rows: &[CategoryProposalRow],
    status: CatStatusFilter,
    active_key: Option<&str>,
) -> Markup {
    let active_row = active_key.and_then(|k| rows.iter().find(|r| r.merchant_key == k));
    let queue = render_queue(state, rows, active_key, status);
    let detail = match active_row {
        Some(row) => {
            let is_skipped = state.categorise.decisions.get(&row.merchant_key) == Some(&Decision::Skip);
            let title = category_title(&state.conn, row.proposed_category);
            render_detail(row, title.as_deref(), is_skipped)
        }
        None => html! {
            div.empty-state {
                p { "No proposals to show. Run `categorise scan` to populate the staging table." }
            }
        },
    };
    let activity = render_activity(state);

    let chips = crate::freshness::header_chips(&state.conn);
    crate::render::render_page_with_chips(
        "categorise",
        "Category Proposals",
        chips,
        queue,
        detail,
        activity,
    )
}

fn render_queue(
    state: &AppState,
    rows: &[CategoryProposalRow],
    active_key: Option<&str>,
    status: CatStatusFilter,
) -> Markup {
    let decisions = &state.categorise.decisions;
    html! {
        div.queue-header {
            h2 { (rows.len()) " proposals" }
            div.filter-row {
                @for f in &CatStatusFilter::ALL {
                    button.filter-btn
                        .(if *f == status { "active" } else { "" })
                        hx-get=(format!("/categorise/queue?filter={}", f.as_str()))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.as_str().to_uppercase()) }
                }
                @let n_skipped = count_decisions(decisions, Decision::Skip);
                @if n_skipped > 0 && status == CatStatusFilter::Skipped {
                    button.filter-btn.clear-skipped-btn
                        hx-post="/categorise/clear-all-skipped"
                        hx-target="body"
                    { "CLEAR SKIPPED (" (n_skipped) ")" }
                }
            }
        }
        div.queue-list {
            @for row in rows {
                @let enc = encode_key(&row.merchant_key);
                @let is_active = active_key == Some(row.merchant_key.as_str());
                @let session = decisions.get(&row.merchant_key).copied();
                @let eff = effective_status(row.status, session);
                @let cat = category_title(&state.conn, row.proposed_category);
                div.queue-item
                    .(if is_active { "selected" } else { "" })
                    .((row_status_css(eff)))
                    hx-get=(format!("/categorise/item/{enc}"))
                    hx-target="#detail"
                    hx-swap="innerHTML"
                    data-detail-url=(format!("/categorise/item/{enc}"))
                    data-detail-target="#detail"
                {
                    @if session == Some(Decision::Skip) {
                        span.status-indicator.skip-indicator
                            hx-post=(format!("/categorise/item/{enc}/unskip"))
                            hx-target="body"
                            title="Click to unskip"
                            onclick="event.stopPropagation()"
                        { "\u{2298}" }
                    } @else if eff == Some(Status::Confirmed) {
                        span.status-indicator.confirm-indicator
                            hx-post=(format!("/categorise/item/{enc}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2713}" }
                    } @else if eff == Some(Status::Rejected) {
                        span.status-indicator.reject-indicator
                            hx-post=(format!("/categorise/item/{enc}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2717}" }
                    } @else {
                        span.conf-badge { (row.txn_count) }
                    }
                    span.payee { (cat.as_deref().unwrap_or("(unmapped)")) }
                    span.gap { (row.proposed_labels.join(",")) }
                }
            }
        }
    }
}

fn effective_status(stored: Status, session: Option<Decision>) -> Option<Status> {
    match session {
        Some(Decision::Confirm) => Some(Status::Confirmed),
        Some(Decision::Reject) => Some(Status::Rejected),
        _ => Some(stored),
    }
}

fn row_status_css(s: Option<Status>) -> &'static str {
    match s {
        Some(Status::Confirmed) => "decided-confirmed",
        Some(Status::Rejected) => "decided-rejected",
        _ => "",
    }
}

fn render_detail(row: &CategoryProposalRow, title: Option<&str>, is_skipped: bool) -> Markup {
    let enc = encode_key(&row.merchant_key);
    let action_base = format!("/categorise/item/{enc}");
    html! {
        div.detail-header {
            h2 {
                (title.unwrap_or("(unmapped)"))
                @if row.status != Status::Pending {
                    " " span.status-badge.((match row.status {
                        Status::Confirmed => "status-confirmed",
                        Status::Rejected => "status-rejected",
                        _ => "",
                    })) {
                        @match row.status {
                            Status::Confirmed => { "\u{2713}" },
                            Status::Rejected => { "\u{2717}" },
                            _ => {},
                        }
                    }
                }
                @if is_skipped { " " span.status-badge { "(skipped this session)" } }
            }
            div.confidence-reason {
                "place type: " (row.place_type.as_deref().unwrap_or("\u{2014}"))
                " \u{00b7} " (row.txn_count) " transaction(s)"
            }
        }

        div.comparison {
            div.txn-cards {
                div.txn-card {
                    div.txn-card-header { span.card-label { "MERCHANT" } }
                    div.txn-card-body {
                        div.field { span.field-label { "Key" } span.field-value { (row.merchant_key) } }
                    }
                }
                div.txn-card {
                    div.txn-card-header { span.card-label { "PROPOSED" } }
                    div.txn-card-body {
                        div.field { span.field-label { "Category" } span.field-value { (title.unwrap_or("(unmapped)")) } }
                        div.field {
                            span.field-label { "Labels" }
                            span.field-value {
                                @if row.proposed_labels.is_empty() { "\u{2014}" }
                                @else { (row.proposed_labels.join(", ")) }
                            }
                        }
                    }
                }
            }
        }

        (crate::render::render_actions(&action_base, is_skipped))
    }
}

fn render_activity(state: &AppState) -> Markup {
    let confirmed_in_db = count_confirmed_in_db(&state.conn);
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (count_decisions(&state.categorise.decisions, Decision::Confirm)) } }
            span.stat { "Rejected " span.count-rejected { (count_decisions(&state.categorise.decisions, Decision::Reject)) } }
            span.stat { "Skipped " span.count-skipped { (count_decisions(&state.categorise.decisions, Decision::Skip)) } }
            span.stat { "Undone " span.count-undone { (state.categorise.undone) } }
            span.stat { "Applied " span.count-applied { (state.categorise.applied) } }
            button.apply-btn
                hx-post="/categorise/apply"
                hx-target="body"
                disabled[confirmed_in_db == 0]
                title=(if confirmed_in_db == 0 { "No confirmed proposals to apply" } else { "Write category + labels for every confirmed proposal and drain the staging row" })
            { "Apply confirmed (" (confirmed_in_db) ")" }
        }
        div.activity-list {
            @for entry in state.categorise.activity.iter().rev().take(20) {
                @let enc = encode_key(&entry.merchant_key);
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
                    span { (entry.category_title) }
                    span { (entry.txn_count) " txn" }
                    @if entry.decision == Decision::Skip {
                        button.undo-btn hx-post=(format!("/categorise/item/{enc}/unskip")) hx-target="body" { "unskip" }
                    } @else {
                        button.undo-btn hx-post=(format!("/categorise/item/{enc}/undo")) hx-target="body" { "undo" }
                    }
                }
            }
        }
    }
}

fn count_confirmed_in_db(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM category_proposals WHERE status = 1",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_key_roundtrips_including_spaces() {
        let key = "greenway meat strathfield nsw";
        let enc = encode_key(key);
        assert!(!enc.contains(' '));
        assert_eq!(decode_key(&enc).as_deref(), Some(key));
    }

    #[test]
    fn decode_rejects_malformed() {
        assert_eq!(decode_key("xyz"), None); // odd length
        assert_eq!(decode_key("zz"), None); // non-hex
    }
}
