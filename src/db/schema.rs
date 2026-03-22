pub(crate) const SCHEMA: &str = "
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
";
