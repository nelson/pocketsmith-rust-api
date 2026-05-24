# serve-normalised — implementation plan

Branch: `feature/serve-normalised`

## Goals

1. Add a normalisation review UI to `bin/serve`, mirroring the transfer review flow.
2. Introduce a `payee_normalisations` staging table (matches the `transfer_pairs` paradigm).
3. Convert `bin/normalise` and `bin/transfers` to a scan/apply paradigm (drop `--dry-run`, `--no-auto`).
4. Reuse CSS/JS/layout/bulk-action patterns from the transfer view.

## Schema

```sql
CREATE TABLE payee_normalisations (
    original_payee  TEXT PRIMARY KEY,
    proposed_payee  TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,   -- 16-char xxh3 hex of original_payee
    class           TEXT,                   -- merchant/person/employer/other or NULL
    features_json   TEXT NOT NULL,
    txn_count       INTEGER NOT NULL,
    status          INTEGER NOT NULL DEFAULT 0 REFERENCES statuses(id),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

Reuses the `statuses` lookup table (0=pending, 1=confirmed, 2=rejected). Skip is session-only.

## Scan policy (per user decisions)

For each unique `original_payee` in `transactions`:

| existing row | proposed vs existing | proposed vs current payee | action                                  |
|--------------|----------------------|---------------------------|-----------------------------------------|
| None         | n/a                  | equal                     | skip (rule a)                           |
| None         | n/a                  | different                 | INSERT pending                          |
| Some(any)    | equal                | n/a                       | UPDATE txn_count only                   |
| Some(any)    | different            | n/a                       | overwrite to pending (F3)               |

## Apply

`apply_confirmed`: write `transactions.payee` for confirmed rows, delete those rows. Rejected rows persist.

## CLI

- `normalise` (scan), `normalise --apply` (drain).
- `transfers` (scan; no auto-confirm of High), `transfers --apply` (drain).
- Removed: `--dry-run`, `--no-auto`.

## Reject semantics

Apply leaves `transactions.payee` untouched. Rejected proposals stay in the table to suppress
re-prompting until a rule change produces a different proposed string.

## Server

One binary, two modes under `/transfers/...` and `/normalise/...`. `/` redirects to `/transfers/`.
Tab bar at top of every page.

Item slug for URLs = 16-char lowercase hex of XXH3-64 hash of original_payee. Stored as
`payee_normalisations.slug`; URL routing does `WHERE slug = ?`.

## Test plan (TDD red-green per commit)

| # | Commit                                               | New tests |
|---|------------------------------------------------------|-----------|
| 1 | schema + db::payee_normalisations                    | 1 unit    |
| 2 | normalise::scan (+ extract format_payee to lib)      | 8 unit    |
| 3 | normalise::apply_confirmed                           | 3 unit    |
| 4 | refactor bin/normalise.rs                            | 0         |
| 5 | refactor bin/transfers.rs                            | 0         |
| 6 | serve restructure (transfers subdir, tab bar)        | 0         |
| 7 | serve normalise helpers (slug + filters)             | 2 unit    |
| 8 | serve normalise handlers                             | 4 unit    |
| 9 | serve normalise views + routes                       | 0         |
| 10| integration tests                                    | 3 integ   |

**Total: ~21 tests (18 unit + 3 integration).**
