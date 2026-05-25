# Plan: Transactions & Dashboard tabs

Status: **planning / ideation only — no implementation yet.**
Branch: `plan/transactions-and-dashboard-tabs`
Mockups: `/tmp/pocketsmith-mockups/` (open `index.html` for the index)

The user wants two new top-level tabs in the serve UI alongside the existing
`Transfers` and `Normalise` tabs. The new tabs share the visual vocabulary,
keyboard shortcuts, and HTMX swap conventions established by the existing
tabs. Their job is to **make data-cleaning progress visible** and to surface
the next most valuable cleaning work.

---

## 1. Reuse contract (what we copy from existing tabs)

These are non-negotiable so the new tabs feel native:

- `render::render_page(tab_slug, title, queue, detail, activity)` — the
  same three-pane shell (`#queue`, `#detail`, `#activity`).
- `render::render_tab_bar` — extended to four entries:
  `Transfers / Normalise / Transactions / Dashboard`.
- `render::render_actions(action_base, is_skipped)` — only used on the
  Transactions tab, where confirm/reject/skip continue to make sense.
- `js::JS` keyboard model — extended in-place, no replacement:

  | Key            | Existing behaviour     | New tab behaviour                                       |
  |----------------|------------------------|---------------------------------------------------------|
  | `Tab` / `Sh-Tab` | Cycle tab bar         | unchanged — works because both new tabs are real tabs   |
  | `↑` / `↓`      | Move queue selection   | unchanged — both new tabs use `.queue-item`             |
  | `Y` / `N` / `S`| Confirm / reject / skip| Transactions: confirm pair / reject pair / skip txn (see §3); Dashboard: no-op |
  | `U`            | Undo last decision     | Transactions: undo; Dashboard: no-op                    |

  New keys to add (additive, won't affect existing tabs because they have
  no targets to bind to):

  | Key | Action                                                            |
  |-----|-------------------------------------------------------------------|
  | `J` / `K` | aliases for `↓` / `↑` (vim-style, optional)                |
  | `G` / `Sh-G` | jump to first / last queue item                          |
  | `1`–`5` | activate the n-th filter button in the queue header          |
  | `[` / `]` | Dashboard: previous / next month                            |
  | `R` | Transactions: jump to next row needing review (most useful chord) |

- `tab::next_after` and `tab::count_decisions` — reused as-is on
  Transactions.
- CSS variables (`--bg`, `--accent`, `--green`, `--red`, `--yellow`,
  `--magenta`, `--cyan`) — reused; new chart colours derive from them so
  the palette stays cohesive.
- `state::AppState` gets two new optional sub-states:
  `transactions: TxnTabState` and `dashboard: DashTabState` — same
  pattern as the existing `transfers` / `normalise` fields.

---

## 2. Data sources we already have

From `pocketsmith.db`:

- `transactions` (~22k rows, 2020-02 → 2026-05). Has `original_payee`,
  `payee`, `category_id`, `is_transfer`, `needs_review`,
  `amount_in_base_currency`, `transaction_account_id`.
- `payee_normalisations` — staging table the Normalise tab already uses.
  Status 0 pending / 1 confirmed / 2 rejected.
- `transfer_pairs` — populated by the pairing pipeline; status & confidence.
- `categories`, `transaction_accounts`.

Derived signals we can compute cheaply (per-transaction):

| Signal                       | How                                                                                                  |
|------------------------------|------------------------------------------------------------------------------------------------------|
| **Paired (transfer)**        | exists in `transfer_pairs` with `status = 1` (confirmed)                                             |
| **Pair proposed, undecided** | exists in `transfer_pairs` with `status = 0`                                                         |
| **Pair rejected**            | exists in `transfer_pairs` with `status = 2`                                                         |
| **Norm rule matches**        | a row in `payee_normalisations` with the same `original_payee` exists (any status)                   |
| **Norm rule confirmed**      | …with `status = 1`                                                                                   |
| **Norm rule pending review** | …with `status = 0` — this means the pipeline produced a proposal but you haven't reviewed it         |
| **No norm rule**             | no matching row in `payee_normalisations` — needs a new rule                                         |
| **Needs categorisation**     | `category_id IS NULL`                                                                                |
| **Flagged by source**        | `needs_review = 1`                                                                                   |
| **Probable transfer, unpaired** | `is_transfer = 1` but no `transfer_pairs` row                                                     |

The Transactions tab's filter chips and the Dashboard's hygiene panel are
both simple counts/joins over the table above.

---

## 3. New tab: **Transactions**

Goal: **a reverse-chronological transaction river with cleaning-state
visible at a glance.** Side panel = list, detail panel = full transaction
view, activity panel = cleaning progress for the current filter.

### 3.1 Queue panel (`#queue`)

- Reverse chronological. Date headers between days. Virtual-scroll-friendly
  page size (e.g. 200 rows + "load older" link). For an MVP, just paginate.
- Each `.queue-item` shows: date (short), payee (or original_payee if
  payee is null), amount (right-aligned, coloured ±), and a **status
  glyph stack** on the left — same `.status-indicator` element as today.

#### Status glyph encoding (Tufte-style: minimal, dense, monochrome with
one accent per channel)

Two glyph slots per row, left of the payee:

| Slot 1 — pairing state | Glyph | Colour          |
|------------------------|-------|-----------------|
| not a transfer         | (none)| —               |
| pair confirmed         | `⇄`   | green           |
| pair proposed          | `⇄`   | yellow (filled outline) |
| pair rejected          | `⇄`   | dim grey        |
| `is_transfer=1`, unpaired (orphan) | `⇄?` | red — **needs attention** |

| Slot 2 — normalisation state    | Glyph | Colour |
|---------------------------------|-------|--------|
| no rule, payee=original_payee   | `·`   | red — **needs a rule** |
| rule confirmed                  | `✓`   | green  |
| rule pending review             | `~`   | yellow — **needs review** |
| rule rejected                   | `✗`   | dim grey |

The same glyphs are used in chart annotations on the Dashboard, so users
learn one vocabulary.

A third optional cue: row tinted left-border red when **uncategorised**.

### 3.2 Filter row (queue header)

Keyboard-numbered chips (so `1`–`5` activate them):

1. **All** (default)
2. **Needs rule** — payee equals original_payee AND no `payee_normalisations` row
3. **Rule pending** — has a pending normalisation
4. **Rule confirmed** — has a confirmed normalisation (sanity sweep)
5. **Pair pending** — has a `transfer_pairs` row with status=0
6. **Orphan transfer** — `is_transfer=1` and no pair row
7. **Uncategorised** — `category_id IS NULL`

Plus a date-range pill ("last 30d / 90d / YTD / all"), an account
multi-select, and a free-text search over `original_payee`/`payee`/`memo`.

### 3.3 Detail panel (`#detail`)

Same card-style as the existing tabs:

- Header: payee (large), amount (large, ±-coloured), date, account name.
- Sub-header chips: category (or "Uncategorised"), labels, transaction_type.
- **Cleaning state card**: the two-glyph stack from above, expanded into
  human-readable labels with quick-action buttons:
  - "No normalisation rule" → button **[Y] Add rule from this txn** (jumps
    to a pre-filled normalise editor / proposes one from the pipeline trace).
  - "Rule pending review" → buttons **[Y] Confirm / [N] Reject** (reuses
    `render_actions`, action_base = `/normalise/item/<slug>`).
  - "Pair proposed" → buttons **[Y] Confirm / [N] Reject** (action_base =
    `/transfers/pair/<a>-<b>`).
  - "Orphan transfer" → button "Find candidate pair" (re-runs pairing for
    just this txn).
- **Pipeline trace** (reused from normalise tab) showing what the rule
  pipeline did to `original_payee`.
- **Memo / note / labels** raw (read-only for now).
- **Sibling transactions** (same `original_payee`) — last N as a tight
  table, just like the normalise tab's matching list.
- **Pair counterpart** if paired.

`Y` / `N` / `S` / `U` are wired to whichever cleaning action is most
relevant for the current row; the detail panel sets `data-action-base`
to that action. If there's nothing to act on, the keys are no-ops.

### 3.4 Activity panel (`#activity`)

Three live counts, scoped to the current filter:

- "Of the N transactions visible, X are clean, Y need a rule, Z need
  review." (one sentence, like a sparkline of state.)
- A **micro-sparkline** of "needs-cleaning count over the last 12 weeks"
  to show progress.
- A "next priority" hint: link to the most common original_payee with no
  rule (highest leverage — fix one rule, cleans many rows).

---

## 4. New tab: **Dashboard**

Goal: **at-a-glance financial health per month, with data-cleanliness as
a first-class signal.** Reuses the 3-pane layout because that's actually
a great fit for this domain:

- `#queue` = list of months (and quarters/years)
- `#detail` = charts for the selected month
- `#activity` = data-hygiene scorecard for the selected month + global
  "what to clean next" hints

### 4.1 Queue panel: months strip

Reverse-chronological list, one row per month (and toggleable to weeks
or quarters). Each row is a *small multiple* in itself — Tufte's "data
in tables" principle:

```
2026-05   ▂▃▄▅▆▆▅▄▃▂  in  $ 90k   out −$115k   net −$25k   ●●○ ▮▮▮▯▯▯
2026-04   ▁▁▂▃▅▇█▇▅▃  in  $ 71k   out −$220k   net −$149k  ●○○ ▮▮▮▯▯▯
2026-03   ▂▂▃▄▅▆▅▄▃▂  in  $ 71k   out  −$87k   net  −$16k  ●●● ▮▮▮▮▮▯
```

Columns:
- month label
- 30-day **cashflow sparkline** (running net balance shape)
- inflow $, outflow $, net $ — net coloured ±
- **hygiene meter**: three dots = (norm-rules clean, pairs clean, all
  categorised). Filled = clean. Hovering reveals counts.
- **bar fill** = % of month already reviewed.

Selecting a month swaps the detail panel (HTMX). `[` / `]` step months.

### 4.2 Detail panel: charts for the selected month

The detail is a vertically-scrolling stack of charts. Tufte principles
applied throughout:
- High data-ink ratio: thin axes, no chartjunk, no 3D.
- Direct labelling on chart elements (no separate legend where avoidable).
- Small multiples for comparison; never one big complicated chart when
  several small ones convey the same data more honestly.
- Same colour vocabulary as the Transactions tab (green=confirmed/income,
  red=needs-attention/outflow, yellow=pending, dim grey=neutral).

**Chart 1 — Income → Spending Sankey** (the headline chart)

```
   Salary   ─────────╮
   Refunds  ──╮      │
   Interest ─╮│      ├──► Mortgage
             ╰┴──────┼──► Groceries
                     ├──► Eating Out
                     ├──► Bills
                     ├──► Transport
                     ╰──► Surplus / Deficit
```

Three columns: income sources (left) → "this month" (middle) → category
spend (right). Surplus/deficit appears as a flow into a fourth node so
positive months balance visually. Width = $; node order = $-descending.

**Chart 2 — Category share (pie)**

A pie chart for outflow only. Tufte was famously unenthusiastic about
pies; we hedge by:
- showing only top 6 slices, rest as "Other" with a sub-list below,
- direct-labelling each slice with `name $amount (%)`,
- pairing it with a *bar chart twin* to its right showing the same
  data but easier to compare ("small multiples of presentation").

**Chart 3 — Daily cashflow strip** (Tufte-favourite)

For the selected month: one bar per day, above-axis green income,
below-axis red outflow. A faint cumulative net line overlays it. Y-axis
is shared with the previous 3 months shown as ghosted small multiples
above the active month so you can compare day-shapes month-to-month.

**Chart 4 — Cumulative net "savings rate" line**

Per-day cumulative `inflow − outflow` line, with a 12-month moving
average ghost line. A horizontal zero rule. The space between the line
and zero gets a subtle green/red fill.

**Chart 5 — Category small multiples (12-month strip)**

A grid of small line charts, one per category, all with identical y-axes
so amounts are honestly comparable. The selected month is highlighted as
a vertical band on every multiple. This is *the* Tufte chart: dozens of
data points, no chartjunk, instant comparability.

**Chart 6 — Account balance ribbons (optional, toggleable)**

Stacked line per account showing balance over the month. Useful for
spotting "money pooled in offset" vs "credit card racked up".

### 4.3 Activity panel: hygiene scorecard

This is the bridge between Dashboard and Transactions. For the selected
month:

```
Cleanliness 78%   ████████░░
  Categorised      94%   ███████████░
  Norm-rules       72%   ████████░░░░
  Pairs reviewed  100%   ████████████

Top leverage right now:
  ▸ "POS AUTHORISATION XS ESPRESSO ..."  43 txns, no rule  → [Add rule]
  ▸ "AMAZON MARKETPLACE"                 28 txns, rule pending → [Review]
  ▸ 3 orphan transfers between Smart Access ⇄ Offset → [Pair them]
```

Each link deep-links to the Transactions tab pre-filtered to the relevant
slice. That's the loop: dashboard tells you *where* the work is,
Transactions tab is *where you do* it.

---

## 5. Routing / file layout

Mirrors the existing `transfers/` and `normalise/` modules:

```
src/bin/serve/
  main.rs              + new route arms for /transactions/* and /dashboard/*
  tab.rs               (unchanged)
  render.rs            extend render_tab_bar to 4 entries
  js.rs                add J/K/G/[/]/R bindings
  css.rs               add .chart, .sparkline, .pie, .sankey, .small-mult, .hygiene-bar styles

  transactions/
    mod.rs
    handlers.rs        – act/undo/skip (delegates into transfers / normalise)
    helpers.rs         – TxnQueueRow, TxnFilter, status derivation, signals
    views.rs           – render_page_shell / queue / detail / activity

  dashboard/
    mod.rs
    handlers.rs        – mostly GETs; selecting a month is the only real action
    helpers.rs         – month aggregations, sankey/pie data, sparkline data
    views.rs           – render_page_shell / months_queue / month_detail / hygiene
    charts.rs          – pure SVG chart functions (no JS chart library;
                         we draw SVG server-side from `maud`, htmx-friendly,
                         deterministic, easy to test)
```

### Why server-side SVG (no Chart.js / d3)

- Matches the rest of the stack (maud + htmx, no client framework).
- Deterministic: snapshot tests stay simple.
- Tufte-aligned: forces us to make conscious choices about ink, no
  defaults full of gridlines.
- Sankeys: a small custom layout function is ~150 LoC and good enough
  for a fixed three-column sankey. We don't need d3-sankey's generality.

### State

```rust
#[derive(Default)]
pub struct TxnTabState {
    pub active_id: Option<i64>,
    pub filter: TxnFilter,        // chip selection
    pub date_range: DateRange,
    pub account_ids: Vec<i64>,
    pub query: String,
}

#[derive(Default)]
pub struct DashTabState {
    pub selected_month: Option<String>, // "YYYY-MM"
    pub granularity: Granularity,        // Month | Week | Quarter
}
```

---

## 6. Suggested build order

1. **Plumbing**: extend tab bar, route table, AppState; stub
   `/transactions/` and `/dashboard/` returning placeholder pages. Smoke
   test: tab cycling works.
2. **Transactions queue**: render reverse-chrono list with the two-glyph
   status stack. No filters yet.
3. **Transactions detail**: read-only. Use existing `render_actions` when
   the row has something actionable.
4. **Transactions filters & search**: chips, date range, account
   multi-select.
5. **Transactions actions**: wire `Y/N/S/U` to delegate into the
   transfers/normalise handlers based on the row's primary cleaning need.
6. **Dashboard months strip + scorecard** (no charts yet — the hygiene
   panel alone is already useful).
7. **Dashboard charts** in this order, smallest-effort-first:
   sparkline → daily-cashflow strip → category small-multiples → pie →
   cumulative-net line → sankey.
8. **Polish**: keyboard hints overlay (`?` key), URL-deep-linking from
   Dashboard hygiene rows into pre-filtered Transactions tab.

Each step is independently shippable behind the new tab, so we can stop
at any point and still have value.

---

## 7. Open questions for you

These shape the implementation and the user wants to weigh in before we
build:

1. **Default landing tab.** Currently `/` redirects to `/transfers/`.
   When these new tabs ship, should `/` redirect to **Dashboard** (the
   "where to start today" page) or stay on Transfers?
2. **Transactions tab — scope of mutation.** The existing tabs only
   *propose* changes (apply is a separate step). Should the Transactions
   tab let you **edit category / payee / labels directly**, or stay
   strictly read-only and route all mutation through the staging tables?
   I lean read-only-plus-deep-links to keep the data model honest.
3. **"Add rule from this txn."** When you click this on a row with no
   norm rule, do you want:
   (a) auto-create a `payee_normalisations` row in `pending` status
       using the pipeline's current proposal, then jump to the Normalise
       tab focused on it; or
   (b) open a small inline editor on the Transactions tab to author the
       rule by hand?
4. **Multi-currency.** You have one USD account (PayPal USD). Charts
   currently use `amount_in_base_currency`. OK to draw everything in
   AUD-equivalents and add a small footer "values in AUD"? Or keep
   per-currency split?
5. **Date range default for Transactions tab.** Last 90 days, or all?
   (All = 22k rows, fine for SQLite + virtual scroll, but may feel slow
   on first paint.)
6. **Sankey direction.** Left-to-right "income → spend"? Or two-sided
   ("sources → middle → destinations") with refunds/transfers visible
   as crossing flows? I went with the simple LTR for the mockups but
   the symmetric one is more truthful.
7. **Pie chart at all?** I included one for completeness but Tufte
   would skip it. If you agree, the bar-chart twin replaces it entirely.
8. **Granularity.** Months, weeks, or both? Weekly view aligns better
   with paycheck cadence; monthly aligns with bills. I'd ship monthly
   first and add a toggle.
9. **Hygiene metric weighting.** "Cleanliness 78%" — should it weigh
   transactions by absolute amount (so cleaning a $5k txn moves the
   needle more than a $5 txn), by count (each txn equal), or both
   (show two numbers)?
10. **Keyboard chord `R`** ("jump to next row needing review"). Does
    "needing review" mean only norm rules, or any cleaning state? I'd
    have it cycle: pending norm → orphan transfer → uncategorised →
    pending pair, in that priority order.
11. **Dashboard charts: SVG vs canvas.** I'm proposing server-rendered
    SVG. Is that OK or do you want interactivity (hover tooltips, slice
    drill-down)? Hover tooltips work fine with pure SVG + a tiny bit
    of JS; drill-down would push us toward client charts.
12. **Account scope on Dashboard.** Default = all accounts collapsed
    into one cashflow? Or default = exclude offset/loan/stocks accounts
    so the picture matches "spending money"?
