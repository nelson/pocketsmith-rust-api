# pocketsmith-sync

Syncs PocketSmith data to a local SQLite database and provides tools for transaction analysis. Includes CLI binaries for sync / normalisation / transfer detection / push-to-Pocketsmith, and a local web UI for reviewing transfer pairs.

Most data — including review decisions (confirm/reject) — lives locally in `pocketsmith.db`. The `push` binary is the only path that writes back to PocketSmith; it is opt-in and currently limited to confirmed transfer pairs (Stage 1 of the push rollout — see `.claude/plans/push-overview.md`).

## Setup

```
cp .env.example .env  # add your POCKETSMITH_API_KEY
```

## Sync

Pull all transactions, accounts, and categories from PocketSmith into `pocketsmith.db`:

```
cargo run --bin sync
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

### Review (web UI)

To review pending pairs, run the local web server (feature-gated behind `web`):

```
cargo run --bin serve --features web
```

Then open <http://127.0.0.1:3141>. Override the port with `SERVE_PORT=4000 cargo run --bin serve --features web`.

The page is a single HTMX-driven view:

- **Queue (left)** — filterable list of pairs (status: all / pending / confirmed / rejected / skipped, plus confidence: all / high / medium / low). Click a pair to load it.
- **Detail (right)** — side-by-side transaction cards for the selected pair, prior transfer history for the two accounts, and **Y confirm / N reject / S skip** action buttons (also bound to keyboard shortcuts).
- **Activity (bottom)** — running log of decisions made this session with per-row undo, plus confirmed/rejected/skipped/undone counts and a "clear all skipped" action.

Confirm and Reject write straight to the `transfer_pairs.status` column in `pocketsmith.db`. Skip is in-memory only (not persisted) and is forgotten when the server restarts. The web UI does **not** apply confirmed pairs to the `transactions` table — run `cargo run --bin transfers -- --apply` for that step.

This is a graphical alternative to the now-removed `--review` CLI flag.

### Apply

Applies all confirmed pairs - sets `category_id` to `_Transfer` and `is_transfer = 1` on both transactions. Changes are tracked via `_operations` with reason `"transfers"`:

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

## Push to PocketSmith (Stage 1)

Pushes the result of `transfers --apply` back to PocketSmith: for each confirmed transfer pair, a single PUT sets `is_transfer=true` and `category_id=<_Transfer>` on each of the two transactions. Driven entirely from local DB state — no manual ids.

This is **Stage 1** of a staged rollout. Out of scope until later stages: `payee` / `note` / `labels` / `memo` (Stage 3), per-field conflict detection (Stage 4), conflict review UX (Stage 5). See `.claude/plans/push-overview.md` for the stage table and rationale.

### Dry-run

Lists the work without issuing any PUTs:

```
cargo run --bin push -- --dry-run
```

### Apply

```
cargo run --bin push
```

Optional flags:

- `--dry-run` — log `would_push` for each pending txn; issue no PUTs.
- `--limit N` — cap the batch size (useful when first turning this on).

The summary at the end reports five counters: `pushed`, `would_push`, `skipped_changed_upstream`, `deleted_upstream`, `failed`. Exit code is non-zero only on `failed > 0`.

### Safety: timestamp guard

Before each PUT the binary issues a `GET /transactions/{id}` and compares the remote `updated_at` against the local one. If they differ → that txn is skipped (recorded as `skipped_changed_upstream`) and no PUT is sent. This is intentionally blunt; Stage 4 will replace it with a per-field check.

### Audit trail

Every attempt — regardless of outcome — writes a row to the `push_log` table with the request body, response body or error, and both observed `updated_at` timestamps. Inspect with:

```sql
SELECT outcome, COUNT(*) FROM push_log GROUP BY outcome;
SELECT * FROM push_log WHERE outcome = 'failed' OR outcome = 'skipped_changed_upstream';
```

Successful pushes also stamp `_transaction_changes.pushed_at` on every change row whose `mask` includes a Stage-1 bit (`category_id` or `is_transfer`) **and** whose `operation_id` came from `transfers --apply` (reason `'transfers'`). Re-running `push` is a no-op once `pushed_at` is set — the pending query filters those out.

### Architectural invariant: push does not write to `transactions`

`push` writes to `_transaction_changes` (the `pushed_at` column) and to `push_log`, but **never** to the `transactions` table itself. The `transactions` table is the local mirror of remote state managed by `sync`, overlaid with un-pushed local edits from `normalise` / `transfers --apply`; push is neither, so it has no business mutating that table. The server-side `updated_at` returned by the PUT is preserved in `push_log.response_body` for audit, and the next `sync` will naturally pull the bumped value.

A dedicated regression test (`push::tests::push_does_not_modify_transactions_table`) snapshots every column of the affected `transactions` row before and after a push and asserts they're byte-identical.

### Typical workflow

```
cargo run --bin transfers -- --apply   # write local is_transfer + category_id
cargo run --bin push -- --dry-run      # preview
cargo run --bin push                   # actually PUT
cargo run --bin sync                   # pulls the server-bumped updated_at
```

The explicit `sync` after `push` is the supported pattern for keeping the local mirror in step with what the server now reports.

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

Run the pipeline and write normalised payee strings to `transactions.payee`. All changes are tracked via `_operations` with reason `"normalisation"`. Only rows where the payee actually changes are written (unchanged values are skipped to avoid polluting the history table):

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

## Testing

```
cargo test
```
