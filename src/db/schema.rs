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
-- Seed only the masks the current codebase produces (Option A, enumerated).
-- The FK from _transaction_changes.mask -> field_masks.mask will reject any
-- new combination, prompting a deliberate addition here.
INSERT OR IGNORE INTO field_masks (mask, name) VALUES
    ( 0, 'none'),
    ( 1, 'payee'),
    ( 2, 'category_id'),
    ( 4, 'note'),
    ( 8, 'labels'),
    (16, 'is_transfer'),
    (18, 'category_id, is_transfer'),
    (32, 'memo'),
    (63, 'create');

CREATE TABLE IF NOT EXISTS _transaction_change_log (
    version              INTEGER PRIMARY KEY,
    reason               TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    transactions_updated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS _transaction_change_log_context (
    _version INTEGER NOT NULL
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

CREATE TABLE IF NOT EXISTS _transactions_history (
    id              INTEGER PRIMARY KEY,
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
    _version        INTEGER NOT NULL REFERENCES _transaction_change_log(version),
    _updated        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    _mask           INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_transactions_history_transaction_id
    ON _transactions_history(transaction_id);

CREATE TABLE IF NOT EXISTS transfer_pairs (
    txn_id_a    INTEGER NOT NULL REFERENCES transactions(id),
    txn_id_b    INTEGER NOT NULL REFERENCES transactions(id),
    amount_cents INTEGER NOT NULL,
    confidence  INTEGER NOT NULL REFERENCES confidences(id),
    status      INTEGER NOT NULL DEFAULT 0 REFERENCES statuses(id),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(txn_id_a),
    UNIQUE(txn_id_b)
);

CREATE TRIGGER IF NOT EXISTS _transactions_history_insert
AFTER INSERT ON transactions
WHEN NOT EXISTS (SELECT 1 FROM _transactions_history WHERE transaction_id = NEW.id)
BEGIN
    INSERT INTO _transactions_history (transaction_id, payee, category_id, note, labels, is_transfer, memo, _version, _mask)
    VALUES (NEW.id, NEW.payee, NEW.category_id, NEW.note, NEW.labels, NEW.is_transfer, NEW.memo,
            (SELECT _version FROM _transaction_change_log_context), 63);
END;

CREATE TRIGGER IF NOT EXISTS _transactions_history_update
AFTER UPDATE ON transactions
WHEN (OLD.payee IS NOT NEW.payee
   OR OLD.category_id IS NOT NEW.category_id
   OR OLD.note IS NOT NEW.note
   OR OLD.labels IS NOT NEW.labels
   OR OLD.is_transfer IS NOT NEW.is_transfer
   OR OLD.memo IS NOT NEW.memo)
BEGIN
    INSERT INTO _transactions_history (
        transaction_id,
        payee, category_id, note, labels, is_transfer, memo,
        old_payee, old_category_id, old_note, old_labels, old_is_transfer, old_memo,
        _version, _mask
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
        (SELECT _version FROM _transaction_change_log_context),
        (CASE WHEN OLD.payee IS NOT NEW.payee THEN 1 ELSE 0 END)
        | (CASE WHEN OLD.category_id IS NOT NEW.category_id THEN 2 ELSE 0 END)
        | (CASE WHEN OLD.note IS NOT NEW.note THEN 4 ELSE 0 END)
        | (CASE WHEN OLD.labels IS NOT NEW.labels THEN 8 ELSE 0 END)
        | (CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN 16 ELSE 0 END)
        | (CASE WHEN OLD.memo IS NOT NEW.memo THEN 32 ELSE 0 END)
    );
END;
";
