"""SQLite helpers for payee normalisation pipeline."""

from typing import List, Tuple
import sqlite3
import shutil
from contextlib import contextmanager
from pathlib import Path

# SQL extracted from src/db/schema.rs lines 90-153
HISTORY_SCHEMA = """
CREATE TABLE IF NOT EXISTS _transaction_history_reason (
    reason TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS _transactions_history (
    _rowid       INTEGER NOT NULL,
    payee        TEXT,
    category_id  INTEGER,
    note         TEXT,
    labels       TEXT,
    is_transfer  INTEGER,
    memo         TEXT,
    reason       TEXT NOT NULL,
    _version     INTEGER NOT NULL DEFAULT 1,
    _updated     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    _mask        INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_transactions_history_rowid
    ON _transactions_history(_rowid);

CREATE TRIGGER IF NOT EXISTS _transactions_history_insert
AFTER INSERT ON transactions
WHEN NOT EXISTS (SELECT 1 FROM _transactions_history WHERE _rowid = NEW.id)
BEGIN
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _version, _mask)
    VALUES (NEW.id, NEW.payee, NEW.category_id, NEW.note, NEW.labels, NEW.is_transfer, NEW.memo,
            (SELECT reason FROM _transaction_history_reason), 1, 63);
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
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _version, _mask)
    VALUES (
        NEW.id,
        CASE WHEN OLD.payee IS NOT NEW.payee THEN NEW.payee ELSE NULL END,
        CASE WHEN OLD.category_id IS NOT NEW.category_id THEN NEW.category_id ELSE NULL END,
        CASE WHEN OLD.note IS NOT NEW.note THEN NEW.note ELSE NULL END,
        CASE WHEN OLD.labels IS NOT NEW.labels THEN NEW.labels ELSE NULL END,
        CASE WHEN OLD.is_transfer IS NOT NEW.is_transfer THEN NEW.is_transfer ELSE NULL END,
        CASE WHEN OLD.memo IS NOT NEW.memo THEN NEW.memo ELSE NULL END,
        (SELECT reason FROM _transaction_history_reason),
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
    INSERT INTO _transactions_history (_rowid, payee, category_id, note, labels, is_transfer, memo, reason, _version, _mask)
    VALUES (OLD.id, NULL, NULL, NULL, NULL, NULL, NULL,
            (SELECT reason FROM _transaction_history_reason),
            (SELECT COALESCE(MAX(_version), 0) + 1 FROM _transactions_history WHERE _rowid = OLD.id),
            63);
END;
"""


def clone_db(src_path: str, dest_path: str) -> None:
    """Copy source DB to dest, ensuring history tables and triggers exist."""
    shutil.copy2(src_path, dest_path)
    # Remove WAL/SHM files if they exist (start clean)
    for suffix in ["-wal", "-shm"]:
        p = Path(dest_path + suffix)
        if p.exists():
            p.unlink()
    # Ensure history infrastructure exists
    conn = sqlite3.connect(dest_path)
    conn.execute("PRAGMA journal_mode = WAL;")
    conn.execute("PRAGMA foreign_keys = ON;")
    conn.executescript(HISTORY_SCHEMA)
    conn.close()


@contextmanager
def change_reason(conn: sqlite3.Connection, reason: str):
    """Context manager mirroring Rust's with_change_reason (mod.rs lines 44-52)."""
    conn.execute("DELETE FROM _transaction_history_reason", [])
    conn.execute("INSERT INTO _transaction_history_reason (reason) VALUES (?)", [reason])
    try:
        yield conn
    finally:
        conn.execute("DELETE FROM _transaction_history_reason", [])


def get_all_payees(conn: sqlite3.Connection) -> List[Tuple[int, str, str]]:
    """Return (id, payee, original_payee) for all transactions."""
    cur = conn.execute(
        "SELECT id, payee, original_payee FROM transactions ORDER BY id"
    )
    return [(row[0], row[1] or "", row[2] or "") for row in cur.fetchall()]


def batch_update_payees(conn: sqlite3.Connection, updates: List[Tuple[str, int]]) -> int:
    """Batch update payees. updates = [(new_payee, txn_id), ...]. Returns count of actual changes."""
    changed = 0
    for new_payee, txn_id in updates:
        cur = conn.execute(
            "UPDATE transactions SET payee = ? WHERE id = ? AND payee IS NOT ?",
            [new_payee, txn_id, new_payee],
        )
        changed += cur.rowcount
    return changed


def open_db(path: str) -> sqlite3.Connection:
    """Open a database connection with standard pragmas."""
    conn = sqlite3.connect(path)
    conn.execute("PRAGMA journal_mode = WAL;")
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn
