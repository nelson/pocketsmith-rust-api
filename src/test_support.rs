//! Tiny helpers used by tests across the workspace (lib tests, binary
//! tests in `src/bin/serve/`, and integration tests in `tests/`).
//!
//! These are always-compiled `pub` functions so they can be called from
//! the binary's test targets and integration tests. The body of each is
//! a couple of lines of SQL, so the runtime/binary cost is negligible.
//!
//! Conventionally everything here is named `seed_*` so it's obvious at a
//! call site that the function is test fixture setup.

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::db::{payee_normalisations as pn, with_operation};
use crate::review::Status;

/// Insert a row into `transaction_accounts`. Idempotent across re-calls
/// with the same `id`.
pub fn seed_account(conn: &Connection, id: i64, name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO transaction_accounts (id, name) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
        params![id, name],
    )?;
    Ok(())
}

/// Insert a row into `transactions` with the minimal fields the staging
/// flows care about: `original_payee` (input to the normalise pipeline)
/// and `payee` (what the user currently sees / would be overwritten on
/// apply). Wrapped in `with_operation("test-seed", ...)` so the
/// `_transaction_changes_insert` trigger sees a current operation row.
pub fn seed_txn(
    conn: &Connection,
    id: i64,
    transaction_account_id: i64,
    original_payee: &str,
    payee: &str,
) -> Result<()> {
    with_operation(conn, "test-seed", |conn| {
        conn.execute(
            "INSERT INTO transactions (id, transaction_account_id, date, amount, original_payee, payee)
             VALUES (?1, ?2, '2026-01-01', -10.0, ?3, ?4)",
            params![id, transaction_account_id, original_payee, payee],
        )?;
        Ok(())
    })
}

/// Insert (or overwrite) a row in `payee_normalisations`. Returns the
/// XXH3 slug of the original payee so callers can use it for follow-up
/// `get_by_slug` lookups.
pub fn seed_pn(
    conn: &Connection,
    original_payee: &str,
    proposed_payee: &str,
    status: Status,
    txn_count: i64,
) -> Result<String> {
    let slug = pn::slug_for(original_payee);
    pn::upsert(
        conn,
        &pn::PayeeNormalisationRow {
            original_payee: original_payee.into(),
            proposed_payee: proposed_payee.into(),
            slug: slug.clone(),
            class: Some("merchant".into()),
            features_json: "{}".into(),
            txn_count,
            status,
        },
    )?;
    Ok(slug)
}
