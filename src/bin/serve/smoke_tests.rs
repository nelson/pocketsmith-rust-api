//! HTML smoke tests for the serve binary's view fragments.
//!
//! Both tabs render large HTML pages where regressions are easy to
//! introduce (mistyped class names, missing data attributes, swapped
//! HTMX targets). These tests build an [`AppState`] backed by an
//! in-memory DB with a known fixture, invoke each tab's
//! `render_page_shell` (and a couple of key fragments), and assert
//! that the rendered HTML contains the structural pieces that the
//! JS / HTMX flow depends on.
//!
//! Coverage focus: the things that would break the keyboard navigation
//! and HTMX swap contract — i.e. the integration points between
//! handlers, views, and the JS in [`crate::js`].

use std::sync::{Arc, Mutex};

use pocketsmith_sync::db::{initialize_in_memory, transfer_pairs, with_operation};
use pocketsmith_sync::review::Status;
use pocketsmith_sync::test_support::{seed_account, seed_pn, seed_txn};
use pocketsmith_sync::transfers::{Confidence, TransferPair};

use crate::state::AppState;

fn fresh_state() -> Arc<Mutex<AppState>> {
    let conn = initialize_in_memory().unwrap();
    seed_account(&conn, 1, "Cheque").unwrap();
    seed_account(&conn, 2, "Savings").unwrap();
    // One transfer pair, pending review.
    seed_txn(&conn, 10, 1, "FROM CHEQUE", "FROM CHEQUE").unwrap();
    seed_txn(&conn, 11, 2, "TO SAVINGS", "TO SAVINGS").unwrap();
    with_operation(&conn, "test-seed", |c| {
        transfer_pairs::insert_pair(
            c,
            &TransferPair {
                txn_id_a: 10,
                txn_id_b: 11,
                amount_cents: 5000,
                confidence: Confidence::High,
                status: Status::Pending,
            },
        )
    })
    .unwrap();
    // One pending payee normalisation.
    seed_pn(&conn, "WOOLIES NORTH STRATHF", "Woolworths", Status::Pending, 3).unwrap();
    Arc::new(Mutex::new(AppState::new(conn)))
}

fn contains_all(html: &str, expected: &[&str]) {
    for needle in expected {
        assert!(
            html.contains(needle),
            "expected rendered HTML to contain {:?}\n--- html ---\n{}",
            needle,
            html
        );
    }
}

#[test]
fn transfers_page_renders_tab_bar_and_queue_and_actions() {
    let state = fresh_state();
    let html = crate::views::render_page_shell(&state).into_string();

    contains_all(
        &html,
        &[
            // Tab bar with the right active tab.
            "class=\"tab-bar\"",
            "class=\"tab active\">Transfers",
            "href=\"/normalise/\"",
            // Layout panel ids that the JS and HTMX swap targets rely on.
            "id=\"queue\"",
            "id=\"detail\"",
            "id=\"activity\"",
            // Queue row pointing at the typed detail URL + target.
            "data-detail-url=\"/transfers/pair/10-11\"",
            "data-detail-target=\"#detail\"",
            // Action buttons with the data-action-base used by the
            // keyboard handler in js.rs.
            "data-action-base=\"/transfers/pair/10-11\"",
            "[Y] Confirm",
            "[N] Reject",
            "[S] Skip",
            // Apply control on the activity panel.
            "/transfers/apply",
        ],
    );
}

#[test]
fn normalise_page_renders_tab_bar_and_queue_and_actions_and_trace() {
    let state = fresh_state();
    let html = crate::normalise::views::render_page_shell(&state).into_string();

    contains_all(
        &html,
        &[
            "class=\"tab-bar\"",
            "class=\"tab active\">Normalise",
            "href=\"/transfers/\"",
            // Unified panel ids.
            "id=\"queue\"",
            "id=\"detail\"",
            "id=\"activity\"",
            // The queue should reference the staged proposal.
            "Woolworths",
            "data-detail-target=\"#detail\"",
            // Detail panel has the shared action buttons.
            "data-action-base=\"/normalise/item/",
            "[Y] Confirm",
            "[N] Reject",
            "[S] Skip",
            // The pipeline trace block exists.
            "Pipeline trace",
            // Apply button.
            "/normalise/apply",
        ],
    );
}

#[test]
fn normalise_default_filters_are_all_all() {
    let state = fresh_state();
    let html = crate::normalise::views::render_page_shell(&state).into_string();
    // Both the status row's ALL and the class row's ALL should render as
    // the active filter button (look for the active class adjacent to the
    // ALL-label hx-get URLs the queue fragment serves).
    assert!(html.contains("filter-btn active\" hx-get=\"/normalise/queue?filter=all&amp;class=all\""));
}
