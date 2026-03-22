use pocketsmith_sync::client::PocketSmithClient;
use pocketsmith_sync::models::*;

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
