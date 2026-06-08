// =============================================================================
// Schema conventions (Convention C)
// =============================================================================
//
// 1. Underscore prefix marks tables and columns that are managed by triggers
//    or the operation framework, not directly by application logic.
//    Examples: `_operations`, `_current_operation`, `_transaction_changes`.
//
// 2. Application-readable, application-writable tables have NO prefix --
//    whether locally sourced (e.g. `transfer_pairs`, `push_log`) or remotely
//    sourced (e.g. `transactions`, `categories`, `users`).
//
// 3. Lookup / reference tables have NO prefix (e.g. `statuses`, `confidences`,
//    `field_masks`). They are seeded at init time with `INSERT OR IGNORE`.
//
// 4. Timestamp columns use `created_at` / `updated_at` (no underscore prefix),
//    on both prefixed and non-prefixed tables.
//
// 5. Surrogate primary keys on framework tables use `AUTOINCREMENT` so ids are
//    never reused after a delete (rows are referenced elsewhere by id).
// =============================================================================

pub(crate) const SCHEMA: &str = "
-- Reference / lookup tables (Convention C: no underscore prefix)
CREATE TABLE IF NOT EXISTS statuses (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);
INSERT OR IGNORE INTO statuses (id, name) VALUES (0,'pending'), (1,'confirmed'), (2,'rejected');

CREATE TABLE IF NOT EXISTS confidences (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);
INSERT OR IGNORE INTO confidences (id, name) VALUES (0,'low'), (1,'medium'), (2,'high');

CREATE TABLE IF NOT EXISTS field_masks (
    mask  INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);
-- All 64 valid mask values (0..63) are seeded by `db::seed_field_masks()`
-- at initialise time. With every value present, the FK from
-- `_transaction_changes.mask -> field_masks(mask)` acts as a 0..63 range
-- check while still providing joinable human-readable names for ad-hoc SQL.
-- See `src/db/mod.rs::seed_field_masks` for the naming rules.

CREATE TABLE IF NOT EXISTS _operations (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    reason               TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    transactions_updated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS _current_operation (
    id              INTEGER NOT NULL,
    explicit_writes INTEGER
);

CREATE TABLE IF NOT EXISTS users (
    id                          INTEGER PRIMARY KEY,
    login                       TEXT,
    name                        TEXT,
    email                       TEXT,
    avatar_url                  TEXT,
    beta_user                   INTEGER,
    time_zone                   TEXT,
    week_start_day              INTEGER,
    is_reviewing_transactions   INTEGER,
    base_currency_code          TEXT,
    always_show_base_currency   INTEGER,
    using_multiple_currencies   INTEGER,
    available_accounts          INTEGER,
    available_budgets           INTEGER,
    forecast_last_updated_at    TEXT,
    forecast_last_accessed_at   TEXT,
    forecast_start_date         TEXT,
    forecast_end_date           TEXT,
    forecast_defer_recalculate  INTEGER,
    forecast_needs_recalculate  INTEGER,
    last_logged_in_at           TEXT,
    last_activity_at            TEXT,
    created_at                  TEXT,
    updated_at                  TEXT
);

CREATE TABLE IF NOT EXISTS transaction_accounts (
    id                                  INTEGER PRIMARY KEY,
    name                                TEXT,
    number                              TEXT,
    currency_code                       TEXT,
    account_type                        TEXT,
    current_balance                     REAL,
    current_balance_date                TEXT,
    current_balance_in_base_currency    REAL,
    current_balance_exchange_rate       REAL,
    safe_balance                        REAL,
    safe_balance_in_base_currency       REAL,
    starting_balance                    REAL,
    starting_balance_date               TEXT,
    created_at                          TEXT,
    updated_at                          TEXT
);

CREATE TABLE IF NOT EXISTS categories (
    id              INTEGER PRIMARY KEY,
    title           TEXT,
    colour          TEXT,
    parent_id       INTEGER,
    is_transfer     INTEGER,
    is_bill         INTEGER,
    roll_up         INTEGER,
    refund_behaviour TEXT,
    created_at      TEXT,
    updated_at      TEXT,
    FOREIGN KEY (parent_id) REFERENCES categories(id)
);

CREATE TABLE IF NOT EXISTS transactions (
    id                          INTEGER PRIMARY KEY,
    transaction_type            TEXT,
    payee                       TEXT,
    amount                      REAL,
    amount_in_base_currency     REAL,
    date                        TEXT,
    cheque_number               TEXT,
    memo                        TEXT,
    is_transfer                 INTEGER,
    category_id                 INTEGER,
    note                        TEXT,
    labels                      TEXT,
    original_payee              TEXT,
    upload_source               TEXT,
    closing_balance             REAL,
    transaction_account_id      INTEGER,
    status                      TEXT,
    needs_review                INTEGER,
    created_at                  TEXT,
    updated_at                  TEXT,
    FOREIGN KEY (category_id) REFERENCES categories(id),
    FOREIGN KEY (transaction_account_id) REFERENCES transaction_accounts(id)
);

CREATE TABLE IF NOT EXISTS _transaction_changes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id  INTEGER NOT NULL REFERENCES transactions(id),
    payee           TEXT,
    category_id     INTEGER,
    note            TEXT,
    labels          TEXT,
    is_transfer     INTEGER,
    memo            TEXT,
    old_payee       TEXT,
    old_category_id INTEGER,
    old_note        TEXT,
    old_labels      TEXT,
    old_is_transfer INTEGER,
    old_memo        TEXT,
    operation_id    INTEGER NOT NULL REFERENCES _operations(id),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    mask            INTEGER NOT NULL DEFAULT 0 REFERENCES field_masks(mask),
    pushed_at       TEXT
);

CREATE TABLE IF NOT EXISTS push_log (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    txn_id                   INTEGER NOT NULL,
    attempted_at             TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    outcome                  TEXT NOT NULL CHECK(outcome IN
                               ('pushed','would_push','skipped_changed_upstream','deleted_upstream','failed')),
    local_updated_at_before  TEXT,
    remote_updated_at_seen   TEXT,
    request_body             TEXT,
    response_body            TEXT,
    error_message            TEXT
);
CREATE INDEX IF NOT EXISTS idx_push_log_attempted_at ON push_log(attempted_at);
CREATE INDEX IF NOT EXISTS idx_push_log_txn_id ON push_log(txn_id);

-- Queue ordering for the Transactions tab. The queue panel sorts
-- date DESC, id DESC and limits to 1000; without this index SQLite
-- has to scan + sort the full transactions table on every request,
-- which dominates request latency on a 22k-row DB (~20ms each).
-- Multi-column DESC index lets the planner walk the index and emit
-- rows directly without a sort step.
CREATE INDEX IF NOT EXISTS idx_transactions_date_id
    ON transactions(date DESC, id DESC);

-- Lookup by original_payee. Used by:
--  * normalise::helpers::matching_transactions (sibling-txn list on
--    the detail panel, ~22k rows scanned without this).
--  * The pn LEFT JOIN in filtered_transactions when we fold the per-
--    row state derivation into the main queue query.
-- The trailing date,id columns let the same index satisfy the ORDER
-- BY in matching_transactions without a sort step.
CREATE INDEX IF NOT EXISTS idx_transactions_original_payee
    ON transactions(original_payee, date DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_transaction_changes_transaction_id
    ON _transaction_changes(transaction_id);

CREATE TABLE IF NOT EXISTS transfer_pairs (
    txn_id_a    INTEGER NOT NULL REFERENCES transactions(id),
    txn_id_b    INTEGER NOT NULL REFERENCES transactions(id),
    amount_cents INTEGER NOT NULL,
    confidence  INTEGER NOT NULL REFERENCES confidences(id),
    status      INTEGER NOT NULL DEFAULT 0 REFERENCES statuses(id),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(txn_id_a),
    UNIQUE(txn_id_b)
);

CREATE TABLE IF NOT EXISTS payee_normalisations (
    original_payee  TEXT PRIMARY KEY,
    proposed_payee  TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,
    class           TEXT,
    features_json   TEXT NOT NULL,
    txn_count       INTEGER NOT NULL,
    status          INTEGER NOT NULL DEFAULT 0 REFERENCES statuses(id),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_payee_normalisations_status ON payee_normalisations(status);

-- =====================================================================
-- Editable normalisation rules (editable-rules-v3).
--
-- Each pipeline stage that today lives as an in-code `const` table gets
-- a row-set here, editable from the Pipeline tab. The compiled pipeline
-- behaviour is unchanged; only the source of its tables moves to SQLite.
-- The canonical copy of each table is mirrored to `src/rules/<stage>.sql`
-- (see `src/rules/`). Loaded at startup when a table is empty.
--
-- `note`, `created_at`, `updated_at` on every table. `sort_order` only
-- on the loop / first-match stages where rule order is significant.
-- =====================================================================

CREATE TABLE IF NOT EXISTS _meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- entity-extraction (alphabetical, order doesn't matter at apply time) --
CREATE TABLE IF NOT EXISTS rule_persons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- literal substring (case-insensitive)
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (canonical, pattern)
);

CREATE TABLE IF NOT EXISTS rule_merchants (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- regex source
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (pattern)
);

CREATE TABLE IF NOT EXISTS rule_employers (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- regex source
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (pattern)
);

-- loop stages (sort_order matters) --
CREATE TABLE IF NOT EXISTS rule_prefixes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    gateway     TEXT,
    operation   TEXT,                       -- BankingOperation::display_name() or NULL
    has_account INTEGER NOT NULL DEFAULT 0,
    has_date    INTEGER NOT NULL DEFAULT 0,
    note        TEXT,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS rule_suffixes (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern           TEXT NOT NULL UNIQUE,
    gateway           TEXT,
    operation         TEXT,
    institution       TEXT,
    has_account       INTEGER NOT NULL DEFAULT 0,
    has_date          INTEGER NOT NULL DEFAULT 0,
    has_location      INTEGER NOT NULL DEFAULT 0,
    has_currency_code INTEGER NOT NULL DEFAULT 0,
    has_amount        INTEGER NOT NULL DEFAULT 0,
    note              TEXT,
    sort_order        INTEGER NOT NULL,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS rule_expansions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    canonical   TEXT NOT NULL,
    note        TEXT,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- first-match-wins, grouped by op --
CREATE TABLE IF NOT EXISTS rule_banking_ops (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    operation   TEXT NOT NULL,              -- BankingOperation::display_name()
    pattern     TEXT NOT NULL,              -- regex source
    has_account INTEGER NOT NULL DEFAULT 0,
    note        TEXT,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (operation, pattern)
);

-- aux --
CREATE TABLE IF NOT EXISTS rule_locations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    location    TEXT NOT NULL UNIQUE,
    kind        TEXT NOT NULL DEFAULT 'location' CHECK (kind IN ('location','region')),
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TRIGGER IF NOT EXISTS payee_normalisations_updated_at
AFTER UPDATE ON payee_normalisations
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE payee_normalisations
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE original_payee = NEW.original_payee;
END;

CREATE TRIGGER IF NOT EXISTS transfer_pairs_updated_at
AFTER UPDATE ON transfer_pairs
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at  -- guard against trigger recursion
BEGIN
    UPDATE transfer_pairs
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE txn_id_a = NEW.txn_id_a AND txn_id_b = NEW.txn_id_b;
END;

-- Stamp `updated_at` on any edit to an editable rule table, mirroring the
-- `payee_normalisations` trigger above. Keyed on the `id` PK. Idempotent
-- (`CREATE TRIGGER IF NOT EXISTS`); a trigger-only addition needs no
-- re-seed, so RULES_SCHEMA_VERSION stays at 1.
CREATE TRIGGER IF NOT EXISTS rule_persons_updated_at
AFTER UPDATE ON rule_persons
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_persons
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_merchants_updated_at
AFTER UPDATE ON rule_merchants
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_merchants
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_employers_updated_at
AFTER UPDATE ON rule_employers
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_employers
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_prefixes_updated_at
AFTER UPDATE ON rule_prefixes
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_prefixes
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_suffixes_updated_at
AFTER UPDATE ON rule_suffixes
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_suffixes
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_expansions_updated_at
AFTER UPDATE ON rule_expansions
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_expansions
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_banking_ops_updated_at
AFTER UPDATE ON rule_banking_ops
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_banking_ops
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER IF NOT EXISTS rule_locations_updated_at
AFTER UPDATE ON rule_locations
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE rule_locations
    SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS _transaction_changes_insert
AFTER INSERT ON transactions
WHEN NOT EXISTS (SELECT 1 FROM _transaction_changes WHERE transaction_id = NEW.id)
BEGIN
    INSERT INTO _transaction_changes (transaction_id, payee, category_id, note, labels, is_transfer, memo, operation_id, mask)
    VALUES (NEW.id, NEW.payee, NEW.category_id, NEW.note, NEW.labels, NEW.is_transfer, NEW.memo,
            (SELECT id FROM _current_operation), 63);
END;

CREATE TRIGGER IF NOT EXISTS _transaction_changes_update
AFTER UPDATE ON transactions
WHEN (OLD.payee IS NOT NEW.payee
   OR OLD.category_id IS NOT NEW.category_id
   OR OLD.note IS NOT NEW.note
   OR OLD.labels IS NOT NEW.labels
   OR OLD.is_transfer IS NOT NEW.is_transfer
   OR OLD.memo IS NOT NEW.memo)
BEGIN
    INSERT INTO _transaction_changes (
        transaction_id,
        payee, category_id, note, labels, is_transfer, memo,
        old_payee, old_category_id, old_note, old_labels, old_is_transfer, old_memo,
        operation_id, mask
    )
    VALUES (
        NEW.id,
        CASE WHEN OLD.payee IS NOT NEW.payee THEN NEW.payee ELSE NULL END,
        CASE WHEN OLD.category_id IS NOT NEW.category_id THEN NEW.category_id ELSE NULL END,
        CASE WHEN OLD.note IS NOT NEW.note THEN NEW.note ELSE NULL END,
        CASE WHEN OLD.labels IS NOT NEW.labels THEN NEW.labels ELSE NULL END,
        CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN NEW.is_transfer ELSE NULL END,
        CASE WHEN OLD.memo IS NOT NEW.memo THEN NEW.memo ELSE NULL END,
        CASE WHEN OLD.payee IS NOT NEW.payee THEN OLD.payee ELSE NULL END,
        CASE WHEN OLD.category_id IS NOT NEW.category_id THEN OLD.category_id ELSE NULL END,
        CASE WHEN OLD.note IS NOT NEW.note THEN OLD.note ELSE NULL END,
        CASE WHEN OLD.labels IS NOT NEW.labels THEN OLD.labels ELSE NULL END,
        CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN OLD.is_transfer ELSE NULL END,
        CASE WHEN OLD.memo IS NOT NEW.memo THEN OLD.memo ELSE NULL END,
        (SELECT id FROM _current_operation),
        (CASE WHEN OLD.payee IS NOT NEW.payee THEN 1 ELSE 0 END)
        | (CASE WHEN OLD.category_id IS NOT NEW.category_id THEN 2 ELSE 0 END)
        | (CASE WHEN OLD.note IS NOT NEW.note THEN 4 ELSE 0 END)
        | (CASE WHEN OLD.labels IS NOT NEW.labels THEN 8 ELSE 0 END)
        | (CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN 16 ELSE 0 END)
        | (CASE WHEN OLD.memo IS NOT NEW.memo THEN 32 ELSE 0 END)
    );
END;

-- Convention C, part 2 (sync-owned columns):
--   `transactions` is the local mirror of the Pocketsmith state, overlaid
--   with un-pushed local edits. Only six columns are locally writable —
--   the same six tracked by `_transaction_changes.mask`:
--       payee, category_id, note, labels, is_transfer, memo
--   Every other column (id, transaction_type, amount, amount_in_base_currency,
--   date, cheque_number, original_payee, upload_source, closing_balance,
--   transaction_account_id, status, needs_review, created_at, updated_at) is
--   sync-owned: it is set by `db::upsert_transaction` under reason=sync,
--   reflects what Pocketsmith returned, and must never be mutated by
--   normalise / transfers --apply / push / serve / any other local writer.
--
--   The trigger below enforces that invariant: a `BEFORE UPDATE OF <metadata>`
--   on `transactions` raises ABORT unless the current operation's reason is
--   `sync` (the production sync subcommand) or `test` (fixtures that need
--   to construct arbitrary states).
--
--   INSERTs are not gated: the only path that inserts is
--   `db::upsert_transaction`, used by sync (in production) and by tests.
CREATE TRIGGER IF NOT EXISTS _transactions_protect_sync_owned_columns
BEFORE UPDATE OF
    id, transaction_type, amount, amount_in_base_currency, date,
    cheque_number, original_payee, upload_source, closing_balance,
    transaction_account_id, status, needs_review, created_at, updated_at
ON transactions
WHEN (
    SELECT o.reason FROM _operations o
    WHERE o.id = (SELECT id FROM _current_operation)
) NOT IN ('sync', 'test')
BEGIN
    SELECT RAISE(ABORT, 'transactions: sync-owned column may only be modified under reason sync or test');
END;
";

#[cfg(test)]
mod tests {
    use crate::db;

    /// The `rule_*_updated_at` triggers stamp `updated_at` on edit but not
    /// on insert. Verified on `rule_merchants` (all eight share one
    /// generated trigger shape).
    #[test]
    fn rule_updated_at_bumps_on_update_not_insert() {
        let conn = db::initialize_in_memory().unwrap();

        // INSERT: both timestamps default to the same `now` (SQLite fixes
        // the value of `'now'` within a single statement).
        conn.execute(
            "INSERT INTO rule_merchants (canonical, pattern) VALUES ('Foo', '(?i)FOO')",
            [],
        )
        .unwrap();
        let (created, updated): (String, String) = conn
            .query_row(
                "SELECT created_at, updated_at FROM rule_merchants WHERE canonical = 'Foo'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(created, updated, "INSERT leaves created_at == updated_at");

        // Seed a distinctly old `updated_at`. Because NEW.updated_at differs
        // from OLD here, the trigger's `WHEN NEW.updated_at = OLD.updated_at`
        // guard is false and it does not fire — the explicit value sticks.
        const OLD: &str = "2000-01-01T00:00:00.000Z";
        conn.execute(
            "UPDATE rule_merchants SET updated_at = ?1 WHERE canonical = 'Foo'",
            [OLD],
        )
        .unwrap();
        let seeded: String = conn
            .query_row(
                "SELECT updated_at FROM rule_merchants WHERE canonical = 'Foo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, OLD, "explicit updated_at write is not clobbered");

        // A content edit (NEW.updated_at == OLD.updated_at) fires the trigger.
        conn.execute(
            "UPDATE rule_merchants SET canonical = 'Bar' WHERE canonical = 'Foo'",
            [],
        )
        .unwrap();
        let bumped: String = conn
            .query_row(
                "SELECT updated_at FROM rule_merchants WHERE canonical = 'Bar'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(bumped, OLD, "UPDATE bumps updated_at to now()");
        assert!(bumped >= created, "bumped timestamp is not older than created_at");
    }
}
