# Transactions tab — follow-ups

> Branch: a single follow-up branch off `master` after PR #21 merges.
> Status: **deferred / open.** None of these are blocking; they're
> the round-up of partially-implemented and explicitly-deferred items
> from the Transactions tab MVP work.

## What's done (PR #21, branch `features/transactions-tab-mvp`)

- Tab bar extended to five tabs in canonical order: Dashboard /
  Transactions / Review / Transfers / Normalise.
- `/transactions/` page with reverse-chronological queue, three-pillar
  status (Pair / Norm / Cat) per row, filter chips, detail panel with
  pipeline trace and sibling transactions, action wiring (Y/N/S/U)
  delegating to existing `/normalise/` and `/transfers/` endpoints.
- Visual vocabulary: shape-distinct emojis per pillar (links / labels
  / files), category as a pill, tooltips on every glyph.
- Activity panel with session counters and undo buttons.
- Workflow correctness: smart "next-row" resolver after actions,
  scroll preservation across HTMX body swaps, undo restores the
  undone item as active across all three tabs.
- Performance overhaul: `idx_transactions_date_id` and
  `idx_transactions_original_payee` indexes; `fetch_by_id` helper;
  per-row state pre-fetched via LEFT JOINs in
  `filtered_transactions` (eliminates the 2000-query N+1). Net 5–10×
  speedup on POSTs end-to-end (~100ms → ~15-25ms).

122 unit + smoke tests on PR #21.

## What's open

### 1. Free-text search bar

Plan §3.2 mentions a "free-text search over `original_payee` /
`payee` / `memo`". Not implemented in PR #21. Smallest sensible
addition:

- Input box in the queue header next to the filter chips.
- Server-side: `TxnFilter::Search(String)` variant, or a separate
  `?q=...` query param composed with the filter.
- SQL: `WHERE (t.original_payee LIKE '%q%' OR t.payee LIKE '%q%' OR
  t.memo LIKE '%q%')` plus the existing filter clause. With the
  `idx_transactions_original_payee` index already in place, payee
  searches are fast; memo isn't indexed (acceptable — memo searches
  will be the rare case).
- Keyboard binding: `/` focuses the search input; `Esc` clears.
- ~80 LoC + 2-3 tests.

### 2. Account multi-select filter

Plan §3.2: "an account multi-select". Currently the queue shows all
accounts. Defer until the user reports it's a problem; with 24
accounts the noise might be tolerable.

If/when shipping:
- Add `txn_account_ids: Vec<i64>` to `AppState`.
- Render a dropdown in the queue header listing all accounts; each
  toggleable; HTMX swap `#queue` on each toggle.
- SQL: `AND t.transaction_account_id IN (?, ?, ?, ...)`.

### 3. Orphan transfer flow (`Pair` / `Not a transfer` / `Snooze`)

Plan §8 originally specified a new `transfer_decisions` table for
this. **Explicitly deferred** in this session — the user observed
the same state can be encoded by reusing `transfer_pairs` (with a
self-pair convention or a nullable `txn_id_b`) plus a
`snooze_until` column, and decided not to commit to either approach
right now.

Decision: **do nothing for now.** The orphan filter exists in the
queue and shows the user which txns look like transfers but lack a
pair. They can pair manually via the Transfers tab. "Not a transfer"
and "Snooze" are not yet available.

When ready to revisit:
- Two design options on the table — see the discussion in PR #21's
  thread on "transfer_decisions vs reusing transfer_pairs".
- Recommendation: Option A (reuse `transfer_pairs` with self-pair
  convention + new `snooze_until` column), but the user wanted to
  defer the decision until they've used what's there long enough
  to feel the gap.

### 4. `/` redirect to `/dashboard/`

When the Dashboard tab lands (see
[`dashboard-tab-mvp.md`](./dashboard-tab-mvp.md)), flip the redirect
in `main.rs`:

```rust
if method == Method::Get && (path == "/" || path.is_empty()) {
    let resp = Response::from_data(Vec::new())
        .with_status_code(302)
        .with_header(Header::from_bytes("Location", "/dashboard/").unwrap());
    //                                              ^^^^^^^^^^^^
    let _ = request.respond(resp);
    return;
}
```

One-line change. Stage with the first Dashboard commit so the
landing page is never broken.

### 5. Multi-currency disclaimer

Charts and queue show amounts in `amount_in_base_currency` (AUD).
The current data has one USD account (PayPal USD). Footer disclaimer
"values in AUD" mentioned in the original plan but not yet rendered.
Trivial when shipping the Dashboard.

### 6. Keyboard hints overlay (`?`)

Plan §9 step 9 (Polish): a keyboard-hints modal triggered by `?`.
Lists every binding and what it does. ~30 LoC of JS + Markup; not
blocking. Defer to a polish pass after Dashboard + Review.

## Open questions still in flight

1. **Snooze duration default** (when orphan flow is built).
   Default 30d, no quick chips for now? Configurable per-decision?
2. **Curate scoring formula refinement.** Round-1 said start with
   raw $-impact. Eventually we may want time-to-decide weighting
   (easy decisions float to the top). Defer until we have user
   feedback that the current ranking is inadequate.
3. **Editable normalisation rules.** Out of scope for v1 (separate
   plan: [`editable-rules-v2.md`](./editable-rules-v2.md)). Trigger:
   when in-code rule edits become weekly, or when non-dev rule
   editing is desired.
