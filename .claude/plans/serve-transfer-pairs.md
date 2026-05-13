# `serve` — Design plan

Local HTML interface for reviewing pending transfer pairs detected by
`cargo run --bin transfers`. Replaces the batch-of-16 CLI review skill with a
keyboard-driven web UI that writes to the DB per decision and surfaces prior
pair history for context.

Scope for v1: **transfers only**. Normalise UI is a separate later effort.

---

## Background

Two existing CLI commands generated this work:

### `normalise` (`src/bin/normalise.rs`)
Payee-string pipeline (prefix/suffix strip → abbreviation → person/employer/
merchant detection). Human decisions: for each *unclassified payee*, add a
merchant pattern, person pattern, or skip. Queue = unclassified payees ranked
by transaction count. **Out of scope for v1.**

### `transfers` (`src/bin/transfers.rs`)
Detects candidate transfer pairs (opposite-sign amounts, ≤2-day gap, different
accounts) and stores them in `transfer_pairs` with `status='pending'`. Human
decisions per pair: **Yes / No / Skip**. Useful info per pair: amount, two
dates, two payees, two account names, confidence (high / medium / low).

`transfer_pairs` schema (`src/db/schema.rs:113-122`):
```sql
CREATE TABLE transfer_pairs (
    txn_id_a INTEGER, txn_id_b INTEGER,
    amount_cents INTEGER, confidence TEXT,
    status TEXT DEFAULT 'pending',  -- pending | confirmed | rejected
    created_at TEXT,
    UNIQUE(txn_id_a), UNIQUE(txn_id_b)
);
```

Apply step (`cargo run --bin transfers -- --apply`) reads
`status='confirmed'` rows and tags both transactions with the `_Transfer`
category and `is_transfer=1`.

---

## Chosen design — Split pane, mixed queue (transfers-only)

```
┌─ Pending pairs (86) ────┐ ┌─ Pair detail ──────────────────────────────────────┐
│ Filter: [all ▾]         │ │  Pair #142  ·  HIGH confidence  ·  amount $1,000.00│
│ Sort:   [conf, date ▾]  │ │  ────────────────────────────────────────────────  │
│ ─────────────────────── │ │                                                    │
│▸HIGH  $1,000  04-12 0d  │ │   ┌─ Transaction A ──────┐  ┌─ Transaction B ──┐   │
│ HIGH    $400  04-11 1d  │ │   │ id, date, account,    │  │ id, date, ...     │  │
│ MED     $250  04-09 1d  │ │   │ amount, payee,        │  │                   │  │
│ MED      $80  04-08 0d  │ │   │ original_payee,       │  │                   │  │
│ LOW      $42  03-30 2d  │ │   │ category, is_transfer │  │                   │  │
│ ...                     │ │   └───────────────────────┘  └──────────────────┘   │
│ (80 more)               │ │                                                    │
│                         │ │  Prior pairs Everyday ↔ Saver (last 5):            │
│ [⟳ Detect more]         │ │    2026-03-12  $1,000.00  ✓ confirmed              │
│                         │ │    2026-02-12  $1,000.00  ✓ confirmed              │
│                         │ │    ...                                             │
│                         │ │                                                    │
│                         │ │  [ Y ] confirm   [ N ] reject  [ S ] skip          │
│                         │ │  [ → ] next w/o action                              │
│                         │ │  [ U ] undo last                                    │
└─────────────────────────┘ └────────────────────────────────────────────────────┘
┌─ Session activity ──────────────────────────────────────────────────────────────┐
│  Confirmed 24  ·  Rejected 6  ·  Skipped 11  ·  Undone 1                        │
│  16:42  ✓ confirmed  #128  $1,000.00  Everyday → Saver        [undo]            │
│  16:42  ✗ rejected   #127    $250.00  Visa → Cheq             [undo]            │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Tradeoffs vs. alternatives considered
- **A: Focused single-item card** — fastest per item but can't compare or scan.
- **B: Dense table, batch submit** — mirrors current CLI; less context per row;
  batch failure modes.
- **C (chosen): Split pane with prior-pairs context** — richest context,
  per-item DB writes (safe), keyboard-fast. Most code to build.

---

## Stack

- New binary `src/bin/serve.rs`, **feature-gated** behind `--features web`
  so the core CLI stays lean.
  - `cargo run --bin serve --features web`
- **tiny_http** server on `127.0.0.1:<port>` (localhost only — no auth in v1).
  Synchronous, ~3 transitive deps. No async runtime needed.
- **maud** for HTML templating — compile-time macros, zero runtime deps,
  inline composition. Minimal learning curve over raw `format!()` with
  type safety and proper escaping.
- **HTMX** (served inline or from CDN) for partial swaps. Every action
  returns a fragment; no client state.
- Reuses existing `db::` module and `transfers::` logic. **No schema changes.**

### Why not axum?
axum pulls in tokio + tower + hyper + ~25 transitive deps. For a localhost-only
review tool, tiny_http is sufficient and keeps compile times fast.

---

## Visual design

- **Colour scheme**: Tokyo Night Dark
  - Background: `#1a1b26`
  - Surface/card: `#24283b`
  - Border: `#3b4261`
  - Text primary: `#c0caf5`
  - Text secondary: `#565f89`
  - Accent blue: `#7aa2f7`
  - Green (confirmed): `#9ece6a`
  - Red (rejected): `#f7768e`
  - Yellow (medium conf): `#e0af68`
  - Magenta (low conf): `#bb9af7`
- **Fonts**:
  - Navigation / headings: SF Hello (system fallback: -apple-system, sans-serif)
  - Content / data / amounts: SF Mono (system fallback: ui-monospace, monospace)
- **Responsive**: CSS grid layout that collapses to single column on narrow
  screens (<768px). Queue becomes a collapsible drawer on mobile.

---

## Routes

| Method | Path | Returns |
|---|---|---|
| GET  | `/` | Full page shell (queue + detail + activity panels) |
| GET  | `/queue` | Queue panel fragment |
| GET  | `/pair/:id` | Detail panel fragment for one pair |
| POST | `/pair/:id/confirm` | Sets status='confirmed'; returns next pair + queue + activity row |
| POST | `/pair/:id/reject` | Same with status='rejected' |
| POST | `/pair/:id/skip` | No DB write; advances queue, logs to activity |
| POST | `/pair/:id/undo` | Flips status back to 'pending', refreshes panels |
| POST | `/detect` | Runs `transfers::find_pairs()`, inserts new pending rows, refreshes queue |

---

## Behaviour details

**Queue**
- Default sort: `confidence DESC, date_diff ASC` (matches CLI ordering).
- Filter dropdown: all / high / medium / low.
- **Auto-detect** when pending count < **10**: fire `POST /detect` in the
  background, show spinner in queue header. Threshold via env var.

**Detail pane — prior pairs context**
- Last 5 pairs between Account A ↔ Account B by date, showing date, amount,
  status. Single SQL join `transfer_pairs` + `transactions`.

**Session activity**
- In-memory ring buffer (last ~20). **Process-scoped** for v1 (single-user
  local tool). Each row has an `[undo]` link → `POST /pair/:id/undo`.

**Keyboard** (page-level inline `<script>`, triggers HTMX requests):
- `Y` confirm · `N` reject · `S` skip
- `U` undo last
- `↑/↓` navigate without acting
- `→` next without action

**Auto-advance**: Y/N/S submit and move to the next pending pair in one step.

**Single-row writes**: each decision is one `UPDATE transfer_pairs SET status=…`
through the existing change log. No batch state to lose.

---

## Explicitly NOT in v1
- Auth (localhost-only binding).
- Multi-user / multi-session.
- Bulk-confirm by selection.
- Editing the pair itself (changing which two transactions form a pair).
- Normalise UI.
- The `--apply` step is still run separately from the CLI; web UI only marks
  pairs as confirmed/rejected.

---

## Implementation order

1. Add `web` feature + tiny_http/maud deps to `Cargo.toml`.
2. Stub `src/bin/serve.rs` with tiny_http server + `GET /` shell.
3. Queue fragment + sort/filter, reading existing `transfer_pairs` rows.
4. Detail fragment for selected pair, including prior-pairs query.
5. Action POSTs (confirm/reject/skip) with auto-advance.
6. Session activity panel + undo.
7. `POST /detect` + auto-trigger when queue is low.
8. Keyboard shortcuts.

---

## Side quest: rename package

Rename `pocketsmith-sync` → `sync` in `Cargo.toml` (package name) and
update all `use pocketsmith_sync::` references to `use sync::` across
`src/main.rs` and `src/bin/*.rs`.

---

## Open questions to revisit during implementation
- Exact port / port-selection strategy (env var? auto-pick?).
- Whether to surface a "run --apply now" button or keep that strictly CLI.
- HTMX: inline the JS or fetch from CDN (prefer inline for zero-network-dep).
