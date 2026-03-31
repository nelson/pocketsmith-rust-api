# pocketsmith-sync

Syncs PocketSmith data to a local SQLite database and provides tools for transaction analysis.

## Setup

```
cp .env.example .env  # add your POCKETSMITH_API_KEY
```

## Sync

Pull all transactions, accounts, and categories from PocketSmith into `pocketsmith.db`:

```
cargo run
```

Subsequent runs fetch only transactions updated since the last sync.

## Transfer Pairing

Detects internal transfers between your own accounts - paired transactions with identical amounts (opposite signs), across different accounts, within 2 days. Tags them as `_Transfer` to reduce noise in categorisation.

### Detect

Runs the pairing algorithm, inserts new pairs into the DB, and auto-confirms high-confidence matches:

```
cargo run --bin transfers
```

Use `--no-auto` to insert all pairs as `pending` (no auto-confirm):

```
cargo run --bin transfers -- --no-auto
```

### Review

Interactively review pending pairs. Presents each pair with payee, account, date, and confidence level:

```
cargo run --bin transfers -- --review 10
```

Prompt format:

```
[1/10] $1,000.00 (2026-03-03 -> 2026-03-03) HIGH
  A: Transfer to xx8005 CommBank app          (acct: Savings)
  B: Transfer from xx8820 CommBank app        (acct: Everyday)
  [y]es [n]o [s]kip [q]uit >
```

Pairs are sorted by confidence (high first), then by amount descending.

### Apply

Applies all confirmed pairs - sets `category_id` to `_Transfer` and `is_transfer = 1` on both transactions. Changes are tracked via `_transaction_change_log` with reason `"transfers"`:

```
cargo run --bin transfers -- --apply
```

### Confidence scoring

Each pair is scored based on whether the original payee matches known transfer patterns:

| Level | Meaning |
|-------|---------|
| **high** | Both sides match transfer patterns (e.g. "Transfer to xx8005", "Transfer from xx8820") |
| **medium** | One side matches |
| **low** | Neither side matches (amount/date/account still match) |

### Database

Transfer pairs are stored in the `transfer_pairs` table:

```sql
SELECT tp.confidence, tp.status, COUNT(*)
FROM transfer_pairs tp
GROUP BY tp.confidence, tp.status;
```

Each transaction can appear in at most one pair (enforced by unique constraints on `txn_id_a` and `txn_id_b`).

## Testing

```
cargo test
```
