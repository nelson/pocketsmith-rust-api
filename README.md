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

## Payee Normalisation

Cleans raw bank payee strings (e.g. `"WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026"`) into structured, human-readable payee names (e.g. `"Woolworths Strathfield"`). The pipeline runs in stages: prefix stripping, suffix stripping, abbreviation expansion, then classification (person, employer, merchant, banking operation).

### Dry run

Preview what the normalisation would produce without writing to the database. Prints a summary report showing classification breakdown, merchant coverage metrics, and the top gaps:

```
cargo run --bin normalise -- --dry-run
```

Example output:

```
=== DRY RUN (no DB writes) ===

=== Normalisation Summary ===
Total unique original_payees: 10190
Total transactions: 21353
  Merchant:      1792 unique (3124 txns, 15%)
  Person:         777 unique (1551 txns, 7%)
  Employer:        83 unique (264 txns, 1%)
  Other:          205 unique (1533 txns, 7%)
  Unclassified:  7333 unique (14881 txns, 70%)

=== Merchant Coverage ===
  entity_name extracted: 1792/1792 (100%)
  location extracted:    837/1792 (47%)
  full query (both):     837/1792 (47%)

=== Top Unclassified (by txn count) ===
   1. "TRANSPORTFORNSWTRAVEL SYDNEY" → "TRANSPORTFORNSWTRAVEL SYDNEY" (870 txns)
   ...
```

### Apply

Run the pipeline and write normalised payee strings to `transactions.payee`. All changes are tracked via `_transaction_change_log` with reason `"normalisation"`. Only rows where the payee actually changes are written (unchanged values are skipped to avoid polluting the history table):

```
cargo run --bin normalise
```

Formatting rules:
- **Merchants with entity + location**: `"Woolworths Strathfield"`
- **Merchants with entity only**: `"Vodafone Australia"`
- **Non-merchants and unclassified**: uses the cleaned/normalised string from the pipeline

### Iterating

The typical workflow is: dry-run, review the "Top Unclassified" list, add patterns to `src/normalise/merchants.rs` (or `persons.rs`, `employers.rs`), then dry-run again to measure improvement. Repeat until coverage is satisfactory.

## Claude Code Skills

### `/normalise` - Review normalisation gaps

Runs the normalise binary in dry-run mode, presents coverage metrics, then walks through the top unclassified payees asking which ones need new patterns. Use this to identify the highest-impact payees to classify next:

```
/normalise
```

This is a review-only skill - it does not modify source files or the database.

### `/review-transfers` - Batch review transfer pairs

Presents pending transfer pairs 16 at a time for interactive confirmation. Each pair shows the amount, dates, payees, and accounts. You confirm (yes), reject (no), or skip each one, then the decisions are applied in bulk:

```
/review-transfers
```

Loops automatically until all pending pairs are reviewed or you choose to stop.

## Testing

```
cargo test
```
