use anyhow::Result;
use rusqlite::Connection;

use crate::client::PocketSmithClient;
use crate::db;
use crate::models::TransactionParams;

pub fn pull(client: &PocketSmithClient, conn: &Connection) -> Result<()> {
    let user = client.get_me()?;
    println!("User: {} (id={})", user.name.as_deref().unwrap_or("?"), user.id);
    db::upsert_user(conn, &user)?;
    let user_id = user.id;

    let ta_list = client.get_transaction_accounts(user_id)?;
    println!("{} transaction accounts", ta_list.len());
    for ta in &ta_list {
        db::upsert_transaction_account(conn, ta)?;
    }

    let categories = client.get_categories(user_id)?;
    println!("{} top-level categories", categories.len());
    for cat in &categories {
        db::upsert_category(conn, cat)?;
    }

    let last_sync = db::get_last_sync(conn)?;
    let updated_since = last_sync.as_ref().map(|(_, ts)| ts.clone());
    let params = TransactionParams {
        updated_since,
        ..Default::default()
    };

    let label = if last_sync.is_some() { "updated" } else { "all" };
    println!("Fetching {} transactions...", label);
    let transactions = client.get_all_transactions(user_id, &params)?;
    println!("{} transactions fetched", transactions.len());

    db::with_change_reason(conn, "pocketsmith", |conn| {
        let tx = conn.unchecked_transaction()?;
        for txn in &transactions {
            db::upsert_transaction(&tx, txn)?;
        }
        tx.commit()?;
        Ok(())
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    let version = db::insert_sync(conn, &now, transactions.len() as i64)?;
    println!("Sync complete (version={}, last_sync={})", version, now);

    Ok(())
}
