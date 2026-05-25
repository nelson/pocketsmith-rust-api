use pocketsmith_sync::client::PocketSmithClient;
use pocketsmith_sync::db::{self, with_operation};
use pocketsmith_sync::models::*;
use pocketsmith_sync::push::{self, PushOpts};

fn make_client() -> PocketSmithClient {
    let key = std::env::var("POCKETSMITH_API_KEY")
        .expect("POCKETSMITH_API_KEY must be set for integration tests");
    PocketSmithClient::new(key)
}

fn get_user_id(client: &PocketSmithClient) -> i64 {
    client.get_me().expect("get_me failed").id
}

// --- GET smoke tests ---

#[test]
#[ignore]
fn test_get_me() {
    let client = make_client();
    let user = client.get_me().unwrap();
    assert!(user.id > 0);
    assert!(user.login.is_some());
}

#[test]
#[ignore]
fn test_get_user() {
    let client = make_client();
    let user_id = get_user_id(&client);
    let user = client.get_user(user_id).unwrap();
    assert_eq!(user.id, user_id);
}

#[test]
#[ignore]
fn test_get_transaction_accounts() {
    let client = make_client();
    let user_id = get_user_id(&client);
    let tas = client.get_transaction_accounts(user_id).unwrap();
    assert!(!tas.is_empty(), "expected at least one transaction account");
    assert!(tas[0].id > 0);
}

#[test]
#[ignore]
fn test_get_categories() {
    let client = make_client();
    let user_id = get_user_id(&client);
    let categories = client.get_categories(user_id).unwrap();
    assert!(!categories.is_empty(), "expected at least one category");
}

#[test]
#[ignore]
fn test_get_transaction_by_id() {
    let client = make_client();
    let user_id = get_user_id(&client);
    let page = client
        .get_transactions_page(user_id, &TransactionParams::default(), 1)
        .unwrap();
    assert!(!page.is_empty(), "expected at least one transaction");

    let first_id = page[0].id;
    let txn = client.get_transaction(first_id).unwrap();
    assert_eq!(txn.id, first_id);
}

#[test]
#[ignore]
fn test_get_transactions_updated_since() {
    let client = make_client();
    let user_id = get_user_id(&client);
    let params = TransactionParams {
        updated_since: Some("2020-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let txns = client.get_transactions_page(user_id, &params, 1).unwrap();
    assert!(
        !txns.is_empty(),
        "expected transactions updated since 2020"
    );
}

// --- Transaction lifecycle test (create, update, verify, delete) ---

struct CleanupGuard<'a> {
    client: &'a PocketSmithClient,
    txn_id: Option<i64>,
}

impl<'a> Drop for CleanupGuard<'a> {
    fn drop(&mut self) {
        if let Some(id) = self.txn_id {
            let _ = self.client.delete_transaction(id);
        }
    }
}

#[test]
#[ignore]
fn test_transaction_lifecycle() {
    let client = make_client();
    let user_id = get_user_id(&client);

    // Get a transaction account to create the transaction in
    let tas = client.get_transaction_accounts(user_id).unwrap();
    let ta_id = tas[0].id;

    // Get two categories for testing category update
    let categories = client.get_categories(user_id).unwrap();
    assert!(
        categories.len() >= 2,
        "need at least 2 categories for lifecycle test"
    );
    let cat_id_1 = categories[0].id;
    let cat_id_2 = categories[1].id;

    // 1. CREATE a dummy transaction
    let create = TransactionCreate {
        payee: "TDD Dummy Transaction".to_string(),
        amount: -1.23,
        date: "2025-01-01".to_string(),
        memo: Some("original memo".to_string()),
        note: Some("original note".to_string()),
        is_transfer: Some(false),
        category_id: Some(cat_id_1),
        labels: None,
    };
    let created = client.create_transaction(ta_id, &create).unwrap();
    assert!(created.id > 0);

    // Set up cleanup guard so transaction is deleted even if we panic
    let mut guard = CleanupGuard {
        client: &client,
        txn_id: Some(created.id),
    };

    assert_eq!(created.payee.as_deref(), Some("TDD Dummy Transaction"));
    assert_eq!(created.memo.as_deref(), Some("original memo"));

    // 2. UPDATE the transaction (memo, payee, category, is_transfer, note)
    let update = TransactionUpdate {
        memo: Some("updated memo".to_string()),
        payee: Some("Updated Payee".to_string()),
        category_id: Some(cat_id_2),
        is_transfer: Some(true),
        note: Some("updated note".to_string()),
        ..Default::default()
    };
    let updated = client.update_transaction(created.id, &update).unwrap();
    assert_eq!(updated.memo.as_deref(), Some("updated memo"));
    assert_eq!(updated.payee.as_deref(), Some("Updated Payee"));
    assert_eq!(updated.is_transfer, Some(true));
    assert_eq!(updated.note.as_deref(), Some("updated note"));
    assert_eq!(updated.category.as_ref().map(|c| c.id), Some(cat_id_2));

    // 3. GET to verify the update persisted
    let fetched = client.get_transaction(created.id).unwrap();
    assert_eq!(fetched.memo.as_deref(), Some("updated memo"));
    assert_eq!(fetched.payee.as_deref(), Some("Updated Payee"));
    assert_eq!(fetched.is_transfer, Some(true));
    assert_eq!(fetched.note.as_deref(), Some("updated note"));
    assert_eq!(fetched.category.as_ref().map(|c| c.id), Some(cat_id_2));

    // 4. DELETE the transaction
    client.delete_transaction(created.id).unwrap();
    guard.txn_id = None; // Disable cleanup guard since we deleted successfully

    // 5. Verify GET returns error (404)
    let result = client.get_transaction(created.id);
    assert!(result.is_err(), "expected 404 after deletion");
}

// --- push pipeline lifecycle (real client, in-memory DB) ---

/// End-to-end: drive `push::push` against the real PocketSmith API for a
/// single dummy transaction. Mirrors what `cargo run --bin push` does for
/// one of the 1000+ pending normalise-apply rows in production, but on a
/// transaction we own and clean up.
///
/// Steps:
///   1. CREATE a dummy txn via the API.
///   2. Mirror it into an in-memory DB under reason='sync' (one row in
///      transactions; no pending push work).
///   3. UPDATE the local payee under reason='normalise-apply' — exactly
///      what `normalise::apply::apply_confirmed` does on production.
///   4. Run `push::push` against the real client.
///   5. GET the txn back from the API and assert the payee landed.
///   6. CleanupGuard deletes the dummy.
#[test]
#[ignore]
fn test_push_normalise_apply_lifecycle() {
    let client = make_client();
    let user_id = get_user_id(&client);

    let tas = client.get_transaction_accounts(user_id).unwrap();
    let ta_id = tas[0].id;

    // 1. CREATE.
    let create = TransactionCreate {
        payee: "TDD Push Dummy RAW".to_string(),
        amount: -2.34,
        date: "2025-01-01".to_string(),
        memo: None,
        note: None,
        is_transfer: Some(false),
        category_id: None,
        labels: None,
    };
    let created = client.create_transaction(ta_id, &create).unwrap();
    let mut guard = CleanupGuard {
        client: &client,
        txn_id: Some(created.id),
    };

    // 2. Mirror locally under reason='sync'. Use an in-memory DB so we
    //    don't pollute the real pocketsmith.db. We INSERT directly rather
    //    than going through `db::upsert_transaction` because the API's
    //    `Transaction` model carries the account as a nested struct,
    //    while the `transactions` table FKs to a separate row.
    let conn = db::initialize_in_memory().unwrap();
    with_operation(&conn, "sync", |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO transaction_accounts (id, name) VALUES (?1, 'live-test')",
            [ta_id],
        )?;
        conn.execute(
            "INSERT INTO transactions
                (id, transaction_account_id, date, amount, original_payee, payee, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                created.id,
                ta_id,
                created.date.as_deref().unwrap_or("2025-01-01"),
                created.amount.unwrap_or(-2.34),
                created.original_payee.as_deref().unwrap_or("TDD Push Dummy RAW"),
                created.payee.as_deref().unwrap_or("TDD Push Dummy RAW"),
                created.updated_at.as_deref().expect("server must return updated_at"),
            ],
        )?;
        Ok(())
    })
    .unwrap();

    // 3. Simulate `normalise --apply` writing a cleaned payee.
    with_operation(&conn, "normalise-apply", |conn| {
        conn.execute(
            "UPDATE transactions SET payee = 'TDD Push Dummy CLEAN' WHERE id = ?1",
            [created.id],
        )?;
        Ok(())
    })
    .unwrap();

    assert_eq!(
        push::count_pending(&conn).unwrap(),
        1,
        "local apply must produce exactly one pending push row"
    );

    // 4. Push to the real API.
    let stats = push::push(&client, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 1, "stats: {stats:?}");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped_changed_upstream, 0);

    // 5. Re-fetch from the API: the server now has the cleaned payee.
    let fetched = client.get_transaction(created.id).unwrap();
    assert_eq!(fetched.payee.as_deref(), Some("TDD Push Dummy CLEAN"));
    // The other fields the local row didn't touch must be unchanged.
    assert_eq!(fetched.amount, Some(-2.34));
    assert_eq!(fetched.is_transfer, Some(false));

    // Idempotent re-run: nothing left to push.
    let again = push::push(&client, &conn, &PushOpts::default()).unwrap();
    assert_eq!(again.pushed, 0);
    assert_eq!(again.would_push, 0);

    // 6. Cleanup.
    client.delete_transaction(created.id).unwrap();
    guard.txn_id = None;
}
