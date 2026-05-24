mod schema;

pub mod categories;
pub mod payee_normalisations;
pub mod transaction_accounts;
pub mod transactions;
pub mod transfer_pairs;
pub mod users;

pub use categories::upsert_category;
pub use transaction_accounts::upsert_transaction_account;
pub use transactions::{update_payee, upsert_transaction};
pub use users::upsert_user;

use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn initialize(path: &str) -> Result<Connection> {
    let conn = Connection::open(path).context("Failed to open database")?;

    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA).context("Failed to create tables")?;
    drop_legacy_artifacts(&conn)?;
    migrate_add_pushed_at(&conn)?;
    migrate_add_explicit_writes(&conn)?;
    migrate_rename_pocketsmith_reason(&conn)?;
    seed_field_masks(&conn)?;

    Ok(conn)
}

pub fn initialize_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA)?;
    drop_legacy_artifacts(&conn)?;
    migrate_add_pushed_at(&conn)?;
    migrate_add_explicit_writes(&conn)?;
    migrate_rename_pocketsmith_reason(&conn)?;
    seed_field_masks(&conn)?;
    Ok(conn)
}

/// Idempotent migration: add `pushed_at TEXT` to `_transaction_changes` if it
/// is not already there. Fresh DBs already have the column (via SCHEMA) so the
/// PRAGMA check short-circuits. Older DBs (created before the push feature)
/// get the column added in place — no data movement required because the
/// default is NULL.
fn migrate_add_pushed_at(conn: &Connection) -> Result<()> {
    let has_column: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info('_transaction_changes')")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for r in rows {
            if r? == "pushed_at" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE _transaction_changes ADD COLUMN pushed_at TEXT;",
        )?;
    }
    Ok(())
}

/// Idempotent migration: add `explicit_writes INTEGER` to `_current_operation`
/// if absent. Older DBs (created before push) lack this column, so any
/// `with_operation` invocation that tries to read it would otherwise fail.
/// See `with_operation` / `record_operation_writes` for the semantics.
fn migrate_add_explicit_writes(conn: &Connection) -> Result<()> {
    let has_column: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info('_current_operation')")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for r in rows {
            if r? == "explicit_writes" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE _current_operation ADD COLUMN explicit_writes INTEGER;",
        )?;
    }
    Ok(())
}

/// Idempotent migration: the sync subcommand's operation reason was originally
/// 'pocketsmith'. We standardised on "reason = subcommand name" (so
/// `normalise`, `transfers`, `push` all match the binary) and rebadged this
/// one to 'sync'. Rewrite any historical rows so `get_last_change(conn,
/// "sync")` returns the right high-water mark and incremental syncs don't
/// re-pull the world.
fn migrate_rename_pocketsmith_reason(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE _operations SET reason = 'sync' WHERE reason = 'pocketsmith'",
        [],
    )?;
    Ok(())
}

/// Drop artifacts left behind by older schema versions. The legacy
/// `_transactions_history` table and its triggers were replaced by
/// `_transaction_changes` + `_transaction_change_log`; leaving the old
/// triggers in place causes any INSERT/UPDATE on `transactions` to fail with
/// a NOT NULL constraint on `_transactions_history._version` (because the
/// companion `_transaction_change_log_context` row is never populated).
fn drop_legacy_artifacts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS _transactions_history_insert;
         DROP TRIGGER IF EXISTS _transactions_history_update;
         DROP INDEX   IF EXISTS idx_transactions_history_transaction_id;
         DROP TABLE   IF EXISTS _transactions_history;
         DROP TABLE   IF EXISTS _transaction_change_log_context;
         DROP TABLE   IF EXISTS _transaction_change_log;",
    )?;
    Ok(())
}

/// Field bits in ascending order. Index = bit position; value = field name.
/// The bit for each field is `1 << index`. Total bits must be <= 6 (mask is
/// stored as INTEGER and the `_transaction_changes.mask` FK ranges over 0..63).
const FIELD_BITS: [&str; 6] = [
    "payee",        // bit 0 = 1
    "category_id",  // bit 1 = 2
    "note",         // bit 2 = 4
    "labels",       // bit 3 = 8
    "is_transfer",  // bit 4 = 16
    "memo",         // bit 5 = 32
];

/// Compute the canonical name for a mask value. Special cases:
///   0  -> "none"
///   63 -> "create"          (every bit set, as emitted by the INSERT trigger)
/// Otherwise: comma-joined field names in ascending bit order, e.g.
///   18 -> "category_id, is_transfer".
fn mask_name(mask: u8) -> String {
    match mask {
        0 => "none".to_string(),
        63 => "create".to_string(),
        m => FIELD_BITS
            .iter()
            .enumerate()
            .filter(|(i, _)| m & (1 << i) != 0)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Seed `field_masks` with all 64 valid mask values (0..63) and their derived
/// names. Idempotent: re-running on an existing DB is a no-op thanks to
/// `INSERT OR IGNORE`. Called from both `initialize` and `initialize_in_memory`
/// so every connection sees a fully-populated lookup before the FK on
/// `_transaction_changes.mask` is exercised.
fn seed_field_masks(conn: &Connection) -> Result<()> {
    let mut stmt =
        conn.prepare("INSERT OR IGNORE INTO field_masks (mask, name) VALUES (?1, ?2)")?;
    for mask in 0u8..64 {
        stmt.execute(rusqlite::params![mask, mask_name(mask)])?;
    }
    Ok(())
}

pub fn get_last_change(conn: &Connection, reason: &str) -> Result<Option<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at FROM _operations WHERE reason = ?1 ORDER BY id DESC LIMIT 1",
    )?;
    Ok(stmt.query_row([reason], |row| Ok((row.get(0)?, row.get(1)?))).ok())
}

pub fn with_operation<F, T>(conn: &Connection, reason: &str, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    // The whole operation runs inside a single SQLite transaction so that, on
    // failure, both the inner work AND the `_operations` high-water-mark row
    // are rolled back together. Previously the INSERT into `_operations`
    // happened outside any transaction, so a failure inside `f` would leave
    // the high-water mark advanced even though nothing was actually saved
    // (causing subsequent incremental syncs to skip the unsaved range).
    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO _operations (reason) VALUES (?1)",
        [reason],
    )?;
    let version = tx.last_insert_rowid();

    tx.execute("DELETE FROM _current_operation", [])?;
    tx.execute(
        "INSERT INTO _current_operation (id) VALUES (?1)",
        [version],
    )?;

    let result = f(conn)?;

    // Prefer an explicit write-count if the closure called
    // `record_operation_writes` (the way `push` reports its successful PUTs).
    // Otherwise fall back to counting distinct transactions that triggered a
    // `_transaction_changes` row — the right answer for sync / transfers /
    // normalise, where every meaningful local mutation goes through the
    // update trigger.
    let explicit: Option<i64> = tx.query_row(
        "SELECT explicit_writes FROM _current_operation",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    let count: i64 = match explicit {
        Some(n) => n,
        None => tx.query_row(
            "SELECT COUNT(DISTINCT transaction_id) FROM _transaction_changes WHERE operation_id = ?1",
            [version],
            |row| row.get(0),
        )?,
    };
    tx.execute(
        "UPDATE _operations SET transactions_updated = ?1 WHERE id = ?2",
        rusqlite::params![count, version],
    )?;

    tx.execute("DELETE FROM _current_operation", [])?;

    tx.commit()?;
    Ok(result)
}

/// Override the write count that `with_operation` will record on the
/// currently-running operation. Use this from inside a `with_operation`
/// closure when the closure's mutations don't go through the
/// `_transaction_changes` update trigger (notably: `push`, which doesn't
/// modify `transactions` at all and so wouldn't otherwise show any writes).
///
/// Idempotent within an operation — last call wins.
pub fn record_operation_writes(conn: &Connection, n: i64) -> Result<()> {
    conn.execute(
        "UPDATE _current_operation SET explicit_writes = ?1",
        [n],
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::models::*;

    pub fn test_db() -> Connection {
        initialize_in_memory().unwrap()
    }

    pub fn make_user(id: i64, name: &str) -> User {
        User {
            id,
            login: Some("testlogin".into()),
            name: Some(name.into()),
            email: Some("test@example.com".into()),
            avatar_url: None,
            beta_user: Some(false),
            time_zone: Some("UTC".into()),
            week_start_day: Some(1),
            is_reviewing_transactions: Some(false),
            base_currency_code: Some("NZD".into()),
            always_show_base_currency: Some(false),
            using_multiple_currencies: Some(false),
            available_accounts: Some(10),
            available_budgets: Some(5),
            forecast_last_updated_at: None,
            forecast_last_accessed_at: None,
            forecast_start_date: None,
            forecast_end_date: None,
            forecast_defer_recalculate: Some(false),
            forecast_needs_recalculate: Some(false),
            last_logged_in_at: None,
            last_activity_at: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_transaction_account(id: i64, name: &str) -> TransactionAccount {
        TransactionAccount {
            id,
            name: Some(name.into()),
            number: Some("12-3456-7890".into()),
            currency_code: Some("NZD".into()),
            account_type: Some("bank".into()),
            current_balance: Some(1000.0),
            current_balance_date: Some("2024-01-01".into()),
            current_balance_in_base_currency: Some(1000.0),
            current_balance_exchange_rate: Some(1.0),
            safe_balance: Some(900.0),
            safe_balance_in_base_currency: Some(900.0),
            starting_balance: Some(0.0),
            starting_balance_date: Some("2020-01-01".into()),
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_category(id: i64, title: &str) -> Category {
        Category {
            id,
            title: Some(title.into()),
            colour: Some("#ff0000".into()),
            children: None,
            parent_id: None,
            is_transfer: Some(false),
            is_bill: Some(false),
            roll_up: Some(false),
            refund_behaviour: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_transaction(id: i64, payee: &str) -> Transaction {
        Transaction {
            id,
            transaction_type: Some("debit".into()),
            payee: Some(payee.into()),
            amount: Some(-50.0),
            amount_in_base_currency: Some(-50.0),
            date: Some("2024-06-15".into()),
            cheque_number: None,
            memo: None,
            is_transfer: Some(false),
            category: None,
            note: None,
            labels: None,
            original_payee: None,
            upload_source: None,
            closing_balance: None,
            transaction_account: None,
            status: Some("posted".into()),
            needs_review: Some(false),
            created_at: Some("2024-06-15T00:00:00Z".into()),
            updated_at: Some("2024-06-15T00:00:00Z".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_get_last_change_returns_none_when_empty() {
        let conn = test_db();
        assert_eq!(get_last_change(&conn, "sync").unwrap(), None);
    }

    #[test]
    fn test_with_operation_creates_entry() {
        let conn = test_db();
        with_operation(&conn, "sync", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "sync").unwrap().unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_get_last_change_filters_by_reason() {
        let conn = test_db();
        with_operation(&conn, "sync", |_| Ok(())).unwrap();
        with_operation(&conn, "rules", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "sync").unwrap().unwrap();
        assert_eq!(version, 1);
        let (version, _) = get_last_change(&conn, "rules").unwrap().unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn test_with_operation_increments_version() {
        let conn = test_db();
        with_operation(&conn, "test", |_| Ok(())).unwrap();
        with_operation(&conn, "test", |_| Ok(())).unwrap();
        with_operation(&conn, "test", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "test").unwrap().unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn test_with_operation_counts_transactions() {
        let conn = test_db();
        with_operation(&conn, "test", |conn| {
            upsert_transaction(conn, &make_transaction(1, "A"))?;
            upsert_transaction(conn, &make_transaction(2, "B"))?;
            Ok(())
        }).unwrap();
        let count: i64 = conn.query_row(
            "SELECT transactions_updated FROM _operations WHERE id = 1",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_record_operation_writes_overrides_count() {
        let conn = test_db();
        // Closure does nothing that would fire the change trigger, so the
        // default count would be 0. record_operation_writes overrides it.
        with_operation(&conn, "push", |conn| {
            crate::db::record_operation_writes(conn, 7)
        })
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT transactions_updated FROM _operations WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 7);
    }

    #[test]
    fn test_record_operation_writes_last_call_wins() {
        let conn = test_db();
        with_operation(&conn, "push", |conn| {
            crate::db::record_operation_writes(conn, 3)?;
            crate::db::record_operation_writes(conn, 5)?;
            Ok(())
        })
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT transactions_updated FROM _operations WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_explicit_writes_cleared_between_operations() {
        let conn = test_db();
        with_operation(&conn, "push", |conn| {
            crate::db::record_operation_writes(conn, 4)
        })
        .unwrap();
        // A subsequent operation that doesn't call record_operation_writes
        // must NOT inherit the previous run's override — the column lives on
        // _current_operation, which is wiped between runs.
        with_operation(&conn, "test", |conn| {
            upsert_transaction(conn, &make_transaction(1, "A"))?;
            Ok(())
        })
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT transactions_updated FROM _operations WHERE id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "second op should count its own change row, not inherit");
    }

    #[test]
    fn test_migrate_rename_pocketsmith_reason() {
        let conn = test_db();
        // Simulate an older DB that has historical 'pocketsmith' rows by
        // inserting one directly, then running the migration.
        conn.execute(
            "INSERT INTO _operations (reason) VALUES ('pocketsmith')",
            [],
        )
        .unwrap();
        super::migrate_rename_pocketsmith_reason(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _operations WHERE reason = 'pocketsmith'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _operations WHERE reason = 'sync'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        // Idempotent: re-run is a no-op.
        super::migrate_rename_pocketsmith_reason(&conn).unwrap();
    }

    #[test]
    fn test_initialize_creates_all_tables() {
        let conn = test_db();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"transaction_accounts".to_string()));
        assert!(tables.contains(&"categories".to_string()));
        assert!(tables.contains(&"transactions".to_string()));
    }

    #[test]
    fn test_initialize_creates_transfer_pairs_table() {
        let conn = test_db();
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='transfer_pairs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }
}
