pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS _sync_history (
    version              INTEGER PRIMARY KEY,
    synced_at            TEXT NOT NULL,
    transactions_updated INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS _transaction_change_context (
    reason        TEXT NOT NULL,
    _sync_version INTEGER
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
    _rowid        INTEGER NOT NULL,
    payee         TEXT,
    category_id   INTEGER,
    note          TEXT,
    labels        TEXT,
    is_transfer   INTEGER,
    memo          TEXT,
    reason        TEXT NOT NULL,
    _sync_version INTEGER,
    _version      INTEGER NOT NULL DEFAULT 1,
    _updated      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    _mask         INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_transactions_history_rowid
    ON _transactions_history(_rowid);

CREATE TRIGGER IF NOT EXISTS _transactions_history_insert
AFTER INSERT ON transactions
WHEN NOT EXISTS (SELECT 1 FROM _transactions_history WHERE _rowid = NEW.id)
BEGIN
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _sync_version, _version, _mask)
    VALUES (NEW.id, NEW.payee, NEW.category_id, NEW.note, NEW.labels, NEW.is_transfer, NEW.memo,
            (SELECT reason FROM _transaction_change_context),
            (SELECT _sync_version FROM _transaction_change_context), 1, 63);
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
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _sync_version, _version, _mask)
    VALUES (
        NEW.id,
        CASE WHEN OLD.payee IS NOT NEW.payee THEN NEW.payee ELSE NULL END,
        CASE WHEN OLD.category_id IS NOT NEW.category_id THEN NEW.category_id ELSE NULL END,
        CASE WHEN OLD.note IS NOT NEW.note THEN NEW.note ELSE NULL END,
        CASE WHEN OLD.labels IS NOT NEW.labels THEN NEW.labels ELSE NULL END,
        CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN NEW.is_transfer ELSE NULL END,
        CASE WHEN OLD.memo IS NOT NEW.memo THEN NEW.memo ELSE NULL END,
        (SELECT reason FROM _transaction_change_context),
        (SELECT _sync_version FROM _transaction_change_context),
        (SELECT COALESCE(MAX(_version), 0) + 1 FROM _transactions_history WHERE _rowid = NEW.id),
        (CASE WHEN OLD.payee IS NOT NEW.payee THEN 1 ELSE 0 END)
        | (CASE WHEN OLD.category_id IS NOT NEW.category_id THEN 2 ELSE 0 END)
        | (CASE WHEN OLD.note IS NOT NEW.note THEN 4 ELSE 0 END)
        | (CASE WHEN OLD.labels IS NOT NEW.labels THEN 8 ELSE 0 END)
        | (CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN 16 ELSE 0 END)
        | (CASE WHEN OLD.memo IS NOT NEW.memo THEN 32 ELSE 0 END)
    );
END;

CREATE TRIGGER IF NOT EXISTS _transactions_history_delete
AFTER DELETE ON transactions
BEGIN
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _sync_version, _version, _mask)
    VALUES (OLD.id, NULL, NULL, NULL, NULL, NULL, NULL,
            (SELECT reason FROM _transaction_change_context),
            (SELECT _sync_version FROM _transaction_change_context),
            (SELECT COALESCE(MAX(_version), 0) + 1 FROM _transactions_history WHERE _rowid = OLD.id),
            63);
END;
";
