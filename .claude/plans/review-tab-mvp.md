# Review tab — MVP

> Branch: `review-tab-mvp` (to be created from `master` after the
> Dashboard tab merges).
> Status: **planned, not started.**

The Review is the third of three new top-level tabs in the `serve`
web UI. Build order is Transactions (done in PR #21) → Dashboard
(see [`dashboard-tab-mvp.md`](./dashboard-tab-mvp.md)) → Review.

## Goal

The data-quality workbench. The Dashboard answers "what's happening
with my money?"; the Review tab answers "where is my data wrong, and
what's the highest-leverage thing I can do about it right now?"

Distinct from the existing Transfers and Normalise tabs, which are
queues *of* one specific kind of cleaning work. The Review tab ranks
*all* cleaning work by **$-weighted impact** — a single decision on a
high-volume payee can move the cleanliness score 5–10×.

## Reuse contract

Same three-pane shell. Crucially, the detail panel **embeds existing
fragments** — clicking a "needs rule" row swaps in
`/normalise/item/<slug>` HTML; clicking an "orphan transfer" row swaps
in the existing Transactions detail fragment. Zero new view code
beyond the queue ranking and the scorecard.

## Layout

- **`#queue`** — leverage list, sorted by % impact descending. Each
  row shows: `+12.4% ($)  POS AUTHORISATION XS ESPRESSO ...  (43 txns)
  needs rule`. Clickable; HTMX-swaps `#detail` to the corresponding
  source-tab fragment.
- **`#detail`** — embeds the source tab's existing detail fragment.
  Y/N/S buttons on those fragments POST to the existing endpoints
  (already wired in `features/transactions-tab-mvp` for the Transactions
  detail; the Normalise tab also exposes the same fragment shape).
- **`#activity`** — the **global hygiene scorecard**:
  ```
  Cleanliness 64% ($-weighted)   71% (count)
    Categorised   42% / 78%
    Norm rules    72% / 80%
    Pairs reviewed 100% / 100%
  Streak: 4 days   Goal: 90% by 2026-12 (on track)
  ```

## Leverage scoring

Each candidate gets a score: "if the user decides this one item, how
much does the $-weighted cleanliness % move?"

```
impact_pct(decision) = ($-affected-by-decision) / (total-$-in-window) * 100
```

`$-affected-by-decision`:
- For a `payee_normalisations` row in pending status: sum of
  `abs(amount)` over all transactions sharing the row's
  `original_payee`.
- For an `original_payee` with no `payee_normalisations` row at all:
  same sum, divided by ~confidence the next pipeline run will
  produce a usable proposal (defer that nuance — start with raw $).
- For an orphan transfer: `abs(amount)` of the txn itself.
- For an uncategorised txn: `abs(amount)` of the txn itself.

`total-$-in-window`: sum of `abs(amount)` over the time window
(default: last 90 days). Window is a knob the user can change; for
the v1, hardcode 90d.

Locked decision (round-1 review): start with the simplest formula
above. Don't try to weight by "ease of decision" yet — that's an
optimisation we don't have data to justify.

## File layout

```
src/bin/serve/review/
  mod.rs
  handlers.rs   — GET-only; the queue is computed read-only
  helpers.rs    — leverage scoring, ranked queue assembly,
                  scorecard aggregation
  views.rs      — render_page_shell / render_leverage_queue /
                  render_scorecard
                  (detail panel content delegates to the source
                   tab's existing fragment via HTMX hx-get)
```

## Build order

### Stage 1 — Leverage queue

1. `helpers::leverage_candidates(conn, window_days) -> Vec<Candidate>`
   — single-table-of-unions query (or a small loop of three queries,
   measure first). Returns:
   ```rust
   struct Candidate {
       kind: CandidateKind,    // NeedsRule | RulePending | Orphan |
                               // Uncategorised
       label: String,           // payee or "3 orphan transfers ..."
       n_txns: i64,
       impact_dollars_cents: i64,
       impact_pct_basis_points: i64,  // *10000 for sorting precision
       deep_link: String,       // /normalise/item/<slug>, or
                                // /transactions/txn/<id>
   }
   ```
2. `views::render_leverage_queue` — one row per candidate, sorted
   desc by `impact_pct_basis_points`. Clicking a row HTMX-swaps
   `#detail` from the row's `deep_link`.
3. Route `/review/`. Smoke test: page renders with five tab-bar
   entries, the correct active tab, at least one leverage row.

### Stage 2 — Hygiene scorecard

1. `helpers::hygiene(conn, window_days) -> Hygiene { dollar_pct,
   count_pct, per_pillar: [...] }`. One SQL aggregating both
   numerators (clean) and denominators (total), per pillar.
2. `views::render_scorecard` — counter row with the percentages,
   plus per-pillar bars. Same `.hygiene` CSS the Dashboard months
   strip uses.

### Stage 3 — Streak / goal tracking (optional)

Persists daily snapshots of the cleanliness % so the streak counter
("4 days") and goal projection ("on track") work. New table
`hygiene_snapshots(date PRIMARY KEY, cleanliness_pct)`. Snapshot is
written lazily on Review tab page load if the previous snapshot's
date is before today. No daemon needed.

Defer to v2 if shipping pressure is high; the scorecard is useful
without it.

## Decisions locked

- Hygiene metric: **$-weighted primary, count-weighted in brackets**.
- Account scope: **all accounts** (matches Dashboard).
- Tab name: **Review** (selected by user from the shortlist:
  Curate / Refine / Polish / Tidy / Workshop / Backlog).

## Open questions

1. **Window default.** 90 days for $-impact computation. Adjustable?
   Suggested chips: 30d / 90d / YTD / all.
2. **Streak persistence.** Stage 3 above. Skip for v1?
3. **De-duplication.** A single `original_payee` could appear as
   both "no rule" and "uncategorised". Should the leverage queue
   collapse them, or list separately? My read: list separately
   because the actions are different.

## Architecture note

The Review tab has the smallest implementation footprint of the
three new tabs because it's mostly a *read-only* query layer plus
HTMX deep-links. Most of the Detail panel work was done by PR #21.
Estimated: ~half the LOC of the Transactions tab.
