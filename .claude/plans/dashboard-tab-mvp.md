# Dashboard tab — MVP

> Branch: `dashboard-tab-mvp` (to be created from `master` after the
> `features/transactions-tab-mvp` PR merges).
> Status: **planned, not started.**

The Dashboard is the second of three new top-level tabs in the
`serve` web UI. The first (Transactions) was implemented in
`features/transactions-tab-mvp` (PR #21). The third (Review) has its
own plan: [`review-tab-mvp.md`](./review-tab-mvp.md).

When this lands, `/` should also start redirecting to `/dashboard/`
instead of `/transfers/` — see
[`transactions-tab-mvp-followups.md`](./transactions-tab-mvp-followups.md)
for that one-line follow-up.

## Goal

A single page that answers "what's happening with my money this
month?" in one screen, using the same three-pane shell (queue / detail
/ activity) the other tabs use. Tufte-aligned charts, server-rendered
SVG (no client chart library), keyboard-driven navigation that mirrors
the other tabs.

## Reuse contract (already true on `master` after PR #21 lands)

- `render::render_page(tab_slug, title, queue, detail, activity)` —
  the canonical three-pane shell. The body element carries
  `class="tab-dashboard"` so per-tab CSS scopes itself without
  duplicating selectors.
- `render::render_tab_bar` — already lists all five tabs in the
  canonical order: Dashboard / Transactions / Review / Transfers /
  Normalise.
- The keyboard handler in `js.rs` is tab-agnostic; `↑`/`↓` cycle queue
  rows via `data-detail-url` + `data-detail-target`, `Tab` cycles tabs.
  The Dashboard binds two new keys per the PLAN: `[` / `]` step
  months. (`Y/N/S/U` are no-ops on Dashboard.)
- `state::AppState` gains a small `DashTabState` (selected month,
  granularity = Month | Year). Pattern: same as `txn_filter` /
  `txn_active` already on AppState.

## Data sources

All from the existing tables — no schema changes:

- `transactions` (date, amount, amount_in_base_currency,
  category_id, transaction_account_id, original_payee, is_transfer)
- `categories` (id, title, parent_id)
- `transaction_accounts` (id, name, currency_code)
- `transfer_pairs`, `payee_normalisations` — for the data-quality
  signals on each month (the three-dot hygiene meter).

Aggregations live in `dashboard/helpers.rs` as pure functions over
`Connection` returning small structs the views consume. One SELECT
per chart; the queue render does one composite query for the months
strip.

## File layout (mirrors `transactions/`)

```
src/bin/serve/dashboard/
  mod.rs
  handlers.rs    — mostly GETs; selecting a month is the only action
  helpers.rs     — month/year aggregations, sankey ribbons, sparkline
                   datapoints, hygiene-dot calculations
  views.rs       — render_page_shell / render_months_queue /
                   render_month_detail / render_yearly_detail /
                   render_activity
  charts.rs      — pure SVG rendering of each chart kind. Functions
                   are pure: they take data, return Markup. No DB,
                   no state. Easy to unit-test with snapshot-style
                   assertions on key SVG elements.
```

## Granularity

Locked decision (round-1 review): **Month and Year only.** No weekly,
no quarterly. The `Year` toggle on the months strip flips to a single
12-cell yearly view; `[`/`]` step years instead of months in that mode.

## Build order

Each step is independently shippable. Stop after each for review,
mirroring the pacing used in `features/transactions-tab-mvp`.

### Stage 1 — Months strip + summary line, no charts

Goal: `/dashboard/` reachable, queue shows the most recent N months
with their cashflow numbers and three-dot hygiene meter, detail panel
is a placeholder, activity is a one-liner.

1. `helpers::month_summaries(conn, n_months)` — returns
   `Vec<MonthSummary { ym: String, in_cents, out_cents, n_txns,
   pct_categorised, pct_norm_clean, pct_pair_clean }>`.
   One SQL query that groups by `strftime('%Y-%m', date)`.
2. `views::render_months_queue` — one row per month, with:
   - month label (`2026-04`)
   - 30-day sparkline (running net) — render as inline SVG
     `<polyline points="..."/>` from the daily aggregate
   - in / out / net (compact dollars; reuse `format_dollars_compact`)
   - three hygiene dots (HTML spans with class
     `hyg-dot[ on | warn | bad ]`); CSS in `css.rs` already has the
     dot styling for the Transactions activity panel — extend or
     scope.
3. `views::render_page_shell` — the standard three-pane shell with
   "Select a month" detail and a one-liner activity
   ("Showing N months. Toggle Year on the queue header.").
4. Route `/dashboard/`, `/dashboard/month/<YYYY-MM>` GET
   (sets `dash_active_month`, returns the page shell). HTMX
   `hx-target="#detail"` from the queue rows.
5. Smoke test: page renders with all five tab-bar entries, the
   correct active tab, the months queue list, three hygiene dots
   per row.

After Stage 1 the user can tab over and see the monthly figures
without any of the chart work landing yet.

### Stage 2 — Charts for a selected month

In order, each its own commit:

1. **Daily cashflow strip** (smallest, builds the SVG harness):
   one bar per day, above-zero green = inflow, below-zero red =
   outflow. Faint cumulative-net line overlay. ~30 days, ~50 SVG
   elements. Pure function `charts::daily_cashflow(days: &[Day]) -> Markup`.
2. **Category bar list** (formerly "pie + bar twin"; pie was
   dropped in round-1 review). Top 8 categories by spend, sorted
   desc, each row = name + bar + dollar amount + %. "Other" folds
   the rest with a click-to-expand sub-list.
3. **Cumulative-net line** for the month, with previous month
   ghosted. Single `<polyline>` with stroke; another with
   stroke-dasharray for the ghost.
4. **Small multiples** (Tufte's headline chart): grid of mini
   per-category line charts on shared y-axis. Months horizontal,
   24-month window. Highlight the active month as a vertical band.
5. **Sankey** (most ambitious): three-column symmetric layout
   (sources → middle → destinations). Backflow ribbons for refunds.
   Custom layout function (~100-150 LoC, no d3 dep). The yellow
   "uncategorised" wedge on the destination side flags data quality.
6. **Account balance ribbons** (optional toggleable): stacked-area
   per account over the month. Defer if running long.

Each chart renders into the `#detail` panel for the selected month.
Snapshot-style tests: assert key SVG elements (`<polyline>`,
`<rect>`, expected dollar labels) exist; visual review is the user's.

### Stage 3 — Yearly view

Toggle on the queue header switches Month/Year. Year detail shows
12 monthly cells in a single horizontal strip, plus a cumulative net
line vs prior years. `[`/`]` step years instead of months.

`helpers::year_summary(conn, year)` aggregates the 12 months in one
SQL pass. `views::render_yearly_detail` lays out the strip.

## Tufte principles applied throughout

Locked from round-1 review:

- No pie chart. Horizontal bars instead.
- Direct labelling on chart elements where possible (no separate
  legend).
- Small multiples preferred over composite charts when comparing.
- Thin axes, no gridlines unless they earn their ink.
- Yellow "uncategorised" wedge on the sankey is a data-quality
  signal, not a category.

## Decisions locked

From the round-1 review (see archived `PLAN-transactions-and-dashboard.md`
in this branch's git history if you need the full discussion):

- Pie chart **dropped**.
- Sankey **symmetric three-column with backflows** (round-1
  Mockup E direction, redrawn for readability — see
  `/tmp/pocketsmith-mockups/dash-E-sankey-fullwidth.html` if those
  files are still around).
- Granularity: **Month + Year only.**
- Account scope: **all accounts.** Exclude offset / loan / stocks
  is an open question (§ below).
- Multi-currency: chart all amounts via `amount_in_base_currency`
  (AUD); footer disclaimer "values in AUD". (One USD account in the
  current data; not worth per-currency split until that grows.)

## Open questions for the next session

1. **Default landing.** When this lands, flip the `/` redirect
   from `/transfers/` to `/dashboard/`. One-line change in
   `main.rs`. See [`transactions-tab-mvp-followups.md`](./transactions-tab-mvp-followups.md).
2. **Account scope on Dashboard.** Default = all accounts collapsed?
   Or default = exclude offset/loan/stocks so the picture matches
   "spending money"? My read: ship "all" and let the user toggle in
   a later commit if the noise is annoying.
3. **Yearly view layout.** 12 monthly cells in a single horizontal
   strip (Mockup F), or 12 cells stacked vertically with each cell
   reusing the monthly detail layout? First is denser; second is
   honest about how much screen each month deserves.
4. **Charts: SVG only, or SVG + small bit of JS for hover tooltips?**
   Pure-SVG is simpler and snapshot-testable. Hover tooltips would
   improve the sankey and small multiples — worth ~30 LoC of JS?
5. **Hygiene metric weighting.** $-weighted primary (per round-1
   lock for the Review tab). For the three Dashboard dots, same
   weighting? Or count-weighted is fine given the dots are coarse?

## Mockups

Static HTML mockups were produced in round 1 at
`/tmp/pocketsmith-mockups/` (volatile location; may be gone). The
canonical reference for visual intent in this plan is the round-1
review log which is preserved in PR #20's discussion.
