use anyhow::{Context, Result};

use pocketsmith_sync::client::PocketSmithClient;
use pocketsmith_sync::db;
use pocketsmith_sync::models::TransactionParams;

fn main() -> Result<()> {
    let api_key = std::env::var("POCKETSMITH_API_KEY")
        .context("POCKETSMITH_API_KEY environment variable not set")?;

    let client = PocketSmithClient::new(api_key);
    let conn = db::initialize("pocketsmith.db")?;

    // 1. Fetch current user
    println!("Fetching user...");
    let user = client.get_me()?;
    println!("  user: {} (id={})", user.name.as_deref().unwrap_or("?"), user.id);
    db::upsert_user(&conn, &user)?;
    let user_id = user.id;

    // 2. Fetch accounts
    println!("Fetching accounts...");
    let accounts = client.get_accounts(user_id)?;
    println!("  {} accounts", accounts.len());
    for account in &accounts {
        db::upsert_account(&conn, account)?;
    }

    // 3. Fetch transaction accounts
    println!("Fetching transaction accounts...");
    let ta_list = client.get_transaction_accounts(user_id)?;
    println!("  {} transaction accounts", ta_list.len());
    for ta in &ta_list {
        db::upsert_transaction_account(&conn, ta, None)?;
    }

    // 4. Fetch categories
    println!("Fetching categories...");
    let categories = client.get_categories(user_id)?;
    println!("  {} top-level categories", categories.len());
    for cat in &categories {
        db::upsert_category(&conn, cat)?;
    }

    // 5. Fetch all transactions (paginated)
    println!("Fetching transactions...");
    let transactions = client.get_all_transactions(user_id, &TransactionParams::default())?;
    println!("  {} total transactions", transactions.len());

    // Batch insert in a SQLite transaction for performance
    let tx = conn.unchecked_transaction()?;
    for txn in &transactions {
        db::upsert_transaction(&tx, txn)?;
    }
    tx.commit()?;

    println!("Sync complete!");
    println!("  Users: 1");
    println!("  Accounts: {}", accounts.len());
    println!("  Transaction accounts: {}", ta_list.len());
    println!("  Categories: {}", categories.len());
    println!("  Transactions: {}", transactions.len());

    Ok(())
}
