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
    let html = crate::transfers::views::render_page_shell(&state).into_string();

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

#[test]
fn transactions_page_renders_tab_bar_and_three_panes() {
    let state = fresh_state();
    let html = crate::transactions::views::render_page_shell(&state).into_string();

    contains_all(
        &html,
        &[
            // Tab bar with the right active tab.
            "class=\"tab-bar\"",
            "class=\"tab active\">Transactions",
            // Other tabs render as links.
            "href=\"/dashboard/\"",
            "href=\"/review/\"",
            "href=\"/transfers/\"",
            "href=\"/normalise/\"",
            // Layout panel ids that the JS and HTMX swap targets rely on.
            "id=\"queue\"",
            "id=\"detail\"",
            "id=\"activity\"",
        ],
    );
}

#[test]
fn transactions_page_lists_transactions_tab_as_active_only_once() {
    // Guards the canonical tab bar behaviour for the new tab.
    let state = fresh_state();
    let html = crate::transactions::views::render_page_shell(&state).into_string();
    let active_count = html.matches("class=\"tab active\"").count();
    assert_eq!(
        active_count, 1,
        "exactly one active tab expected, got {active_count}"
    );
}

#[test]
fn transactions_page_renders_filter_chips() {
    // The five filter chips drive the queue narrowing. Each chip must
    // hx-get the queue fragment endpoint with its slug; the active
    // chip carries .filter-btn.active.
    let state = fresh_state();
    let html = crate::transactions::views::render_page_shell(&state).into_string();

    contains_all(
        &html,
        &[
            // Chip labels.
            ">All<",
            ">Needs rule<",
            ">Rule pending<",
            ">Orphan transfer<",
            ">Uncategorised<",
            // Each chip swaps the queue panel.
            "hx-target=\"#queue\"",
            // Default filter is 'all'.
            "filter=all",
            // Active chip class on the default (All).
            "filter-btn active",
        ],
    );
}

#[test]
fn transactions_detail_panel_renders_action_buttons_on_norm_pending_card() {
    // The fresh fixture seeds one pending payee_normalisation for
    // original_payee="WOOLIES NORTH STRATHF" and a txn cloud with id 10
    // (FROM CHEQUE) — let's add a non-transfer txn whose payee matches
    // the seeded pn so the norm-pending card renders with Y/N/S.
    let state = fresh_state();
    {
        let st = state.lock().unwrap();
        pocketsmith_sync::db::with_operation(&st.conn, "test-seed", |c| {
            c.execute(
                "INSERT INTO transactions
                   (id, transaction_account_id, date, amount,
                    original_payee, payee, is_transfer)
                 VALUES (50, 1, '2026-04-01', -10.0, 'WOOLIES NORTH STRATHF',
                         'WOOLIES NORTH STRATHF', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    }
    let html = crate::transactions::views::render_detail_fragment(&state, 50).into_string();

    contains_all(
        &html,
        &[
            // The data-action-base hook the JS keyboard handler reads.
            "data-action-base=\"/transactions/txn/50/norm\"",
            "[Y] Confirm",
            "[N] Reject",
            "[S] Skip",
            // Each button posts to the txn-scoped action URL so the
            // re-render lands back on the Transactions page.
            "hx-post=\"/transactions/txn/50/norm/confirm\"",
            "hx-post=\"/transactions/txn/50/norm/reject\"",
            "hx-post=\"/transactions/txn/50/norm/skip\"",
        ],
    );
}

#[test]
fn transactions_activity_panel_shows_recent_decisions_with_undo_btn() {
    let state = fresh_state();
    {
        // Seed a non-transfer txn whose original_payee matches the
        // pre-seeded pn. Then confirm to push an activity entry.
        let st = state.lock().unwrap();
        pocketsmith_sync::db::with_operation(&st.conn, "test-seed", |c| {
            c.execute(
                "INSERT INTO transactions
                   (id, transaction_account_id, date, amount,
                    original_payee, payee, is_transfer)
                 VALUES (60, 1, '2026-04-01', -10.0, 'WOOLIES NORTH STRATHF',
                         'WOOLIES NORTH STRATHF', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    }
    crate::transactions::handlers::act_norm(&state, 60, crate::state::Decision::Confirm);

    let html = crate::transactions::views::render_page_shell(&state).into_string();
    contains_all(
        &html,
        &[
            // Counter says one confirm.
            "Confirmed <span class=\"count-confirmed\">1",
            // Activity row carries an undo-btn pointing at the txn-scoped endpoint.
            "class=\"undo-btn\"",
            "hx-post=\"/transactions/txn/60/norm/undo\"",
            // The payee text appears in the activity row.
            "WOOLIES NORTH STRATHF",
        ],
    );
}

#[test]
fn transactions_queue_emoji_is_clickable_to_undo_when_decided() {
    let state = fresh_state();
    {
        let st = state.lock().unwrap();
        pocketsmith_sync::db::with_operation(&st.conn, "test-seed", |c| {
            c.execute(
                "INSERT INTO transactions
                   (id, transaction_account_id, date, amount,
                    original_payee, payee, is_transfer)
                 VALUES (61, 1, '2026-04-01', -10.0, 'WOOLIES NORTH STRATHF',
                         'WOOLIES NORTH STRATHF', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    }
    // Confirm the rule (DB status flips to Confirmed for the shared
    // pn row; both txn rows displaying that payee should now show a
    // clickable g-norm-confirmed indicator).
    crate::transactions::handlers::act_norm(&state, 61, crate::state::Decision::Confirm);

    let html = crate::transactions::views::render_page_shell(&state).into_string();
    contains_all(
        &html,
        &[
            // The confirmed row's norm emoji is clickable.
            "class=\"g-norm-confirmed clickable\"",
            // Click triggers a POST to the undo endpoint.
            "hx-post=\"/transactions/txn/61/norm/undo\"",
            // ...with a tooltip that says "click to undo".
            "click to undo",
        ],
    );
}
