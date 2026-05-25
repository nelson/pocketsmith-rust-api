# Plan: Dashboard, Transactions & Review tabs

Status: **planning / ideation only — no implementation yet.**
Branch: `plan/transactions-and-dashboard-tabs`
Mockups: `/tmp/pocketsmith-mockups/` (open `index.html` for the index)

Three new top-level tabs in the serve UI alongside the existing `Transfers`
and `Normalise` tabs:

- **Dashboard** — monthly financial picture (sankey + small multiples + cumulative net).
- **Transactions** — reverse-chronological river with cleaning state visible at a glance.
- **Review** — the data-quality workbench (formerly "hygiene"). Tells you what to clean next.

Final tab order, left-to-right: **Dashboard · Transactions · Review · Transfers · Normalise.**
The `/` redirect goes to **Dashboard** (was: Transfers).

Name for the third tab: **Review** — chosen by the user from the
shortlist (Curate, Refine, Polish, Tidy, Workshop, Backlog).

---

## 1. Reuse contract (what we copy from existing tabs)

These are non-negotiable so the new tabs feel native:

- `render::render_page(tab_slug, title, queue, detail, activity)` — the
  same three-pane shell (`#queue`, `#detail`, `#activity`).
- `render::render_tab_bar` — extended to five entries in this order:
  `Dashboard / Transactions / Review / Transfers / Normalise`.
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
  | `R` | Transactions / Review: jump to next row needing review (priority order: pending norm → orphan transfer → uncategorised → pending pair) |
  | `/` | focus the search input (Transactions only)                        |

- `tab::next_after` and `tab::count_decisions` — reused as-is on
  Transactions.
- CSS variables (`--bg`, `--accent`, `--green`, `--red`, `--yellow`,
  `--magenta`, `--cyan`) — reused; new chart colours derive from them so
  the palette stays cohesive.
- `state::AppState` gets three new optional sub-states:
  `dashboard: DashTabState`, `transactions: TxnTabState`, `review: ReviewTabState`
  — same pattern as the existing `transfers` / `normalise` fields.

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

### Visual vocabulary (locked)

A transaction has up to **three** dimension slots, one per cleaning
pillar:

- **Slot 1** = pair state (transfer pairing)
- **Slot 2** = normalisation-rule state
- **Slot 3** = category state (reserved — becomes the third pillar later)

Each pillar has a coherent shape-family so the emoji itself encodes
*both* dimension and state. There is no positional memory burden, and
because every glyph is shape-distinct from every other glyph, the
vocabulary is readable regardless of colour vision.

| State        | Pair (links)              | Normalise (labels)     | Categorise (files)        |
|--------------|---------------------------|------------------------|---------------------------|
| confirmed    | 🔗 paired                 | 🏷️ rule confirmed       | 🗄️ categorised            |
| pending      | 🔁 pair proposed          | 📝 rule pending review   | 📁 category pending        |
| needs you    | ⚠️ orphan transfer        | ❓ no rule             | 📦 uncategorised           |
| rejected     | ✂️ pair rejected          | 🚫 rule rejected        | 🗑️ category rejected      |
| n/a          | · not a transfer          | · already normalised   | · —                       |

Mnemonic: **Pair = links** (chain, scissors, cycle, warning).
**Normalise = labels** (tag, memo, prohibition, question).
**Categorise = filing** (folder, card index, parcel, bin).

Examples reading the vocabulary:
- `· ❓ 📦` — not a transfer, no normalisation rule, uncategorised
- `🔁 🏷️ 🗄️` — transfer pair pending, normalisation confirmed, categorised
- `⚠️ 📝 📦` — orphan transfer, normalisation pending, uncategorised

**Implementation note for v1:** the Categorise slot is rendered now (`📁`
or `📦` based on whether `category_id` is null), but no Categorise
actions are wired up in v1. Decision verbs and a category staging table
are deferred to the same v2 milestone as editable normalisation rules
(§6). The slot exists so users start associating the position with
the pillar before the actions land.

The Dashboard's three-dot hygiene meter on each month row uses the same
three pillars: dot 1 = pair coverage, dot 2 = norm coverage, dot 3 =
category coverage. Hover labels expose what each dot is.

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

**Mutation policy: staging-only, read-only otherwise.** The Transactions
tab does not directly write to `transactions.payee`, `category_id`, etc.
Any cleaning action goes through one of the existing staging tables
(`payee_normalisations`, `transfer_pairs`) or the new
`transfer_decisions` table (§8). The Transactions tab is therefore an
*entry point* into the same workflows the Normalise/Transfers tabs
expose, just sliced by "the txn I'm currently looking at" rather than
"the next thing in the queue". Same endpoints, different lens.

A practical consequence: there is no "Apply category" button on a row.
Category mutations are out of scope for v1 (open question 2 in the
original plan, now closed: staging-only). If you want to recategorise,
you do it in PocketSmith and re-sync.

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

**Layout consistency rule:** every filter view uses the same template
— the queue panel always shows the same row shape (date · two glyphs ·
payee · amount), and the detail panel always shows: header → cleaning-
state cards → pipeline trace (if relevant) → sibling/related rows. The
"Needs rule" view's grouping by `original_payee` (where date column
becomes a count) is the only deliberate exception, called out visually.

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

### 3.5 Endpoints — reuse, don't duplicate

The Transactions tab does **not** introduce new mutation endpoints. It
calls into existing routes:

- Confirming/rejecting a normalisation proposal from the Transactions
  detail panel POSTs to `/normalise/item/<slug>/{confirm,reject,skip,undo}`.
  These are the same endpoints the Normalise tab uses; the response
  re-renders the *Transactions* page (not Normalise) by pointing
  `hx-target` at the right shell.
- Confirming/rejecting a transfer pair POSTs to
  `/transfers/pair/<a>-<b>/{confirm,reject,skip,undo}` similarly.
- Orphan-transfer decisions go to `/transfer-decisions/<txn_id>/...`
  (new — see §8).

This means the Transactions tab is, in code terms, mostly a *view*
layer over data the existing handlers already know how to mutate.
Keeps the implementation small and avoids drift.

---

## 4. New tab: **Dashboard**

Goal: **at-a-glance financial health per month, with data-cleanliness as
a first-class signal.** Reuses the 3-pane layout because that's actually
a great fit for this domain:

- `#queue` = list of months (and quarters/years)
- `#detail` = charts for the selected month
- `#activity` = data-hygiene scorecard for the selected month + global
  "what to clean next" hints

### 4.0 Granularity — Month and Year only

Monthly is the primary view. Yearly is a secondary view (12 months at
a glance for one year, with `[`/`]` stepping years). Weekly and
quarterly are not implemented — if needed they can be added later, but
start minimal.

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
A toggle at the top switches between **Month** view and **Year**
overview (12 monthly cells side-by-side; same `[`/`]` keys step years).

### 4.2 Detail panel: charts for the selected month

The detail is a vertically-scrolling stack of charts. Tufte principles
applied throughout:
- High data-ink ratio: thin axes, no chartjunk, no 3D.
- Direct labelling on chart elements (no separate legend where avoidable).
- Small multiples for comparison; never one big complicated chart when
  several small ones convey the same data more honestly.
- Same colour vocabulary as the Transactions tab (green=confirmed/income,
  red=needs-attention/outflow, yellow=pending, dim grey=neutral).

**Chart 1 — Money-flow Sankey** (the headline chart)

Three-column symmetric layout (sources → month → destinations):

```
   Salary    ─────────╮               ╭─── Mortgage
   Refunds   ───╮      │               ├─── Eating Out
   Interest  ──╮│      ├─── April ───├─── Groceries
   PayID-in  ─╮││      │   "in"  │    ├─── Bills
            ╰┴┴┴───────┼          │    ├─── Shopping
                    │ (refund)←─┤    ├─── Transport
                    │         │    ├─── PayID-out
                    │         │    ╰─── Uncategorised ⚠
                    ╰──── deficit → offset/credit (yellow back-ribbon)
```

- Width = $.
- Backflows (refunds, transfers between accounts) drawn as thin upward
  ribbons from category column back to centre. They're usually small
  but make exceptions easy to spot.
- An **Uncategorised** node on the right is rendered yellow, not red,
  to signal "data quality" rather than "a category". Cleaning
  reapportions it into the red wedges.
- The **deficit** flow (centre → offset/credit) closes the loop
  visually so positive vs negative months read symmetrically.

**Chart 2 — Category share (horizontal bar)**

No pie. Just a horizontal bar list:
`Mortgage  ██████████  $40,000  56%`. Sorted descending. Top 8 named, rest
folded into "Other" with a sub-list. (Tufte, satisfied.)

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

### 4.3 Activity panel: month summary, not hygiene

The full hygiene scorecard moves to its own top-level **Review** tab
(§5). The Dashboard's activity panel keeps only a single condensed line:
"Apr 2026: 68% clean by $-weight (337 txns) · [open in Review]".

Reasoning: Dashboard is for **insights**. Review is for **improving the
data so insights become more accurate**. They're different jobs and
shouldn't share the bottom of the screen.

---

## 5. Review tab

A dedicated top-level tab whose only job is to surface the next most
valuable cleaning work. Same three-pane shell:

- `#queue` — a prioritised list of "things to clean", **scored by
  $-weighted impact**: how much would the cleanliness % move if you
  decided this one item? High-dollar uncategorised txns score above
  low-dollar ones; high-count merchants without a rule score above
  one-offs. Each row also shows transaction count in brackets so the
  $-weight isn't blind to volume:

  ```
  +12.4% ($)  POS AUTHORISATION XS ESPRESSO ...   (43 txns)   needs rule
   +8.1% ($)  AMAZON MARKETPLACE                  (28 txns)   pending review
   +5.9% ($)  3 orphan transfers Smart Access ⇄ Offset (3)   needs pairing
   +3.7% ($)  142 uncategorised txns over $100    (142)       needs cat-fix
  ```

- `#detail` — the *same* detail view as the corresponding source tab.
  E.g. clicking a "needs rule" row shows the Normalise tab's detail
  fragment in-place; clicking an "orphan transfer" row shows the
  Transactions detail with the candidate-counterparts table. We're
  literally embedding existing fragments — zero new view code beyond
  the queue ranking.

- `#activity` — the global hygiene scorecard:
  ```
  Cleanliness $-weighted: 64% · by-count: 71%
    Categorised       42% ($)   /   78% (count)
    Norm rules        72% ($)   /   80% (count)
    Pairs reviewed   100% ($)   /  100% (count)
  Streak: 4 days  ·  Goal: 90% by 2026-12 (on track)
  ```

This is the page where you start your morning of data-cleaning. The
Dashboard tells you *what's happening with the money*; Review tells you
*what's blocking the truth of that picture*.

## 6. Editable normalisation rules — design options

Status: **out of scope for v1**, but the user wants a path to
editability. The current pipeline is baked into Rust. Below are the
options for moving toward UI-editable rules, with pros and cons. v1 of
the new tabs is read-only-plus-staging; v2 picks one of these.

### Option A — DB-backed dictionaries, baked-in pipeline

The pipeline stages stay in Rust. The *data* each stage consumes moves
to SQLite tables. Examples:

- A `merchant_aliases` table: `(pattern, replacement, class)`.
- A `suburb_suffixes` table: list of strings to strip from the tail.
- A `pos_prefixes` table: list of bank-prefix strings to strip from the head.
- A `known_merchants` table for the classifier dictionary.

The UI lets you add/edit rows in those tables. The pipeline reads them
at startup (or on every run — cheap).

**Pros**
- Pipeline determinism preserved. The *shape* of transformation is in
  code; only the *vocabulary* is in the database.
- Easy to test: pipeline tests stay pure (load fixture dicts).
- Versioned: dictionary tables get the same `_changes` treatment as
  transactions.
- Limited blast radius. A user can't accidentally add a stage that
  breaks everything; they can only extend dictionaries.
- Plays well with multi-user: the rule changes are durable artefacts.

**Cons**
- Doesn't cover "I want a one-off regex for this weird payee". You'd
  need a special escape hatch, which leads to Option C.
- Adding a new *kind* of rule (e.g. "strip emoji") still requires code.

### Option B — Override list (one-off table)

A single `payee_overrides` table: `original_payee → forced_payee`.
Applied as the very last pipeline stage; if a row matches, it wins
unconditionally.

**Pros**
- Trivial to implement (≈ 1 SQL table, ≈ 1 pipeline stage).
- Captures the long tail of weirdos cheaply.
- Easy mental model: "if the pipeline gets it wrong, force the answer".

**Cons**
- Doesn't generalise: each override is a manual one-off, paid for
  forever. No leverage.
- The override table just keeps growing. After a few thousand entries
  it becomes its own data-quality problem.
- Misses the point of having a pipeline: encodes facts as exceptions
  rather than as rules.
- Specifically called out by the user as a concern.

### Option C — Hybrid: dictionaries + tiny override list

Do Option A for the dictionaries. Keep an Option-B-style override table
*as a last resort*, with a UI surface that nudges you toward generalising
overrides into dictionary entries (e.g. "this override has matched 5
rows; want to promote it to a `merchant_aliases` row?").

**Pros**
- Best of both worlds: most cleaning happens by general rule; the
  long-tail escape hatch is acknowledged but kept honest.
- The "promote override → dictionary entry" nudge is a nice UX moment
  and a kind of self-correcting feature.

**Cons**
- Two mental models for the user.
- Slightly more code than A or B alone.

### Option D — Scripted rules (embedded language)

Rules become small expressions in DB rows, evaluated by an embedded
interpreter (Rhai, Lua, etc.).

**Pros**
- Maximum flexibility. New rule *kinds* can be added without code
  changes.

**Cons**
- Significant security surface (sandboxing).
- Hard to debug a year from now. "Why did this rule fire?" needs a
  mini-debugger.
- Big maintenance burden for a single-user tool.
- Pure Rust testing of the pipeline becomes much harder.
- Almost certainly overkill.

### Recommendation

**v1**: do nothing. Pipeline stays in Rust. New tabs are staging-only,
as planned.

**v2 (someday)**: Option C — dictionaries in DB plus a small overrides
escape hatch with a "promote to rule" nudge. This gets us most of the
way to editable without going off the deep end.

The nice property of choosing C *later* is that v1 doesn't constrain
it — we add new tables, stages still match against them, no breaking
changes needed.

## 7. Routing / file layout

Mirrors the existing `transfers/` and `normalise/` modules:

```
src/bin/serve/
  main.rs              + new route arms for the three new tabs;
                       + redirect / → /dashboard/
  tab.rs               (unchanged)
  render.rs            extend render_tab_bar to 5 entries, new order
  js.rs                add J/K/G/[/]/R/  bindings
  css.rs               add .chart, .sparkline, .sankey, .small-mult,
                       .hygiene-bar, .heatmap styles

  dashboard/
    mod.rs
    handlers.rs        – mostly GETs; selecting a month is the only action
    helpers.rs         – month aggregations, sankey ribbons, sparklines
    views.rs           – render_page_shell / months_queue / month_detail / activity
    charts.rs          – pure SVG chart functions (no JS chart library;
                         server-side maud, deterministic, easy to test)

  transactions/
    mod.rs
    handlers.rs        – thin: routes act/undo/skip to existing
                         /normalise/* and /transfers/* endpoints; plus
                         the /transfer-decisions/* endpoints (§8)
    helpers.rs         – TxnQueueRow, TxnFilter, status derivation,
                         search, the priority-of-cleaning-need rule for `R`
    views.rs           – render_page_shell / queue / detail / activity

  review/
    mod.rs
    handlers.rs        – GET-only; ranking happens in helpers
    helpers.rs         – leverage scoring (% impact on $-weighted
                         hygiene if this row's decision flips), priority
                         queue assembly
    views.rs           – render_page_shell / leverage_queue / scorecard;
                         detail panel embeds existing /normalise/item/...
                         and /transactions/... fragments by HTMX swap
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

## 8. "Not a transfer" handling — `transfer_decisions` staging

The orphan-transfer flow has a wrinkle: `is_transfer` is a sync-owned
column (the trigger `_transactions_protect_sync_owned_columns` enforces
this). We can't flip it. So when the user says "this isn't actually a
transfer", we record a *decision* without mutating the source-of-truth
row.

New table:

```sql
CREATE TABLE transfer_decisions (
    txn_id      INTEGER PRIMARY KEY REFERENCES transactions(id),
    decision    TEXT NOT NULL CHECK (decision IN ('not_a_transfer', 'snoozed', 'manual_paired')),
    snooze_until TEXT,           -- ISO date; only meaningful for 'snoozed'
    note         TEXT,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL
);
```

Semantics, ordered most-aggressive first:

- **`not_a_transfer`**: persistent. The orphan filter excludes this txn
  permanently. Review scorecard counts it as resolved.
- **`snoozed`** (with `snooze_until`): hide from the orphan filter
  until that date. Re-surfaces afterward. The user's example — "its
  twin may appear later in a future sync" — is exactly what `snoozed`
  is for: "I don't see the counterpart yet; check back in 30 days".
- **`manual_paired`**: rare; user has manually declared a pair that the
  pairing pipeline didn't propose. Records the txn as paired in
  `transfer_pairs` with confidence='manual'. The decision row exists so
  we know the human (not the pipeline) made it.

UI surface (Mockup 4):

- Detail panel for an orphan offers three buttons:
  - **[Y] Pair with selected candidate** (becomes a `transfer_pairs`
    insert; if no auto-suggested candidate, sets `manual_paired`)
  - **[N] Not a transfer** (persistent decision)
  - **[S] Snooze 30 days** (deferred; reappears later)
  - **[U] Undo** (delete the decision row)

Resurfacing logic: a daily/manual `review-recheck` task scans
`transfer_decisions` and clears expired snoozes. Cheap.

Integration with existing `transfer_pairs`: clean. Pair confirmation
remains the primary signal for "yes paired". `transfer_decisions`
handles the negatives and deferrals that don't fit the pair table.

## 9. Suggested build order

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

Updated step list reflecting the three tabs:

0. Tab-bar + landing change: extend `render_tab_bar` to 5 entries in
   the new order; redirect `/` → `/dashboard/`. Stub Dashboard,
   Transactions, Review as placeholder pages. Smoke-test tab cycling.
1. Dashboard months strip + month summary line in activity panel.
2. Dashboard charts in this order: sparkline → daily cashflow strip →
   category bar (no pie) → cumulative net → small multiples → sankey.
3. Dashboard yearly view (12 months side-by-side, single scroll).
4. Transactions queue + detail (read-only). Adopt the consistent layout
   (date · glyphs · payee · amount; detail = header → cleaning cards →
   trace → siblings).
5. Transactions filter chips, search, account multi-select.
6. Transactions actions: route Y/N/S/U to existing /normalise and
   /transfers endpoints based on the row's primary cleaning need.
7. `transfer_decisions` table + endpoints; orphan-transfer detail panel
   exposes Pair / Not-a-transfer / Snooze.
8. Review tab: leverage scoring, ranked queue, embed existing detail
   fragments. $-weighted hygiene scorecard.
9. Polish: keyboard hints overlay (`?`), URL-deep-linking from Review
   into pre-filtered Transactions.

---

## 10. Decisions locked from review round 1

- Default landing: **Dashboard**.
- Tab order: **Dashboard · Transactions · Review · Transfers · Normalise**.
- Mutation policy: **staging-only** for v1. Editable normalisation
  rules deferred (see §6 design options; recommend Option C for v2).
- Pie chart: **dropped**. Use horizontal bars only.
- Sankey: **symmetric three-column with backflows** (the user picked
  E and asked for it to be redrawn for readability — see redrawn
  Mockup E).
- Hygiene metric: **$-weighted primary, with txn-count in brackets**
  for context ("+12.4% · 43 txns").
- Account scope: **all accounts** for now.
- Granularity: **Month + Year only**. No week, no quarter.
- Hygiene tab name: **Review**.
- `R` priority order: pending norm → orphan transfer → uncategorised →
  pending pair (locked).
- `is_transfer=1` orphans handled via new `transfer_decisions` staging
  table with `not_a_transfer` / `snoozed` / `manual_paired`.

## 11. Open questions still on the table

1. **Multi-currency.** You have one USD account (PayPal USD). Charts
   would use `amount_in_base_currency` (AUD). OK to label everything
   AUD with a footer disclaimer, or split per-currency?
2. **Default date range on Transactions tab.** Last 90 days, or all
   (22k rows)? My preference: all, with a virtual-scroll-style "load
   older" pagination. SQLite + a date index handles this easily.
3. **Charts: SVG only, or SVG + small bit of JS for hover tooltips?**
   Pure-SVG is simpler and snapshot-testable. Hover tooltips would
   improve the sankey and small multiples — worth ~30 LoC of JS?
4. **Yearly view on Dashboard.** 12 monthly cells side-by-side as a
   single scroll, or 12 cells stacked vertically with each cell
   reusing the monthly detail layout? The first is denser; the second
   is honest about how much screen each month deserves. (Mockup F
   shows option 1 — see how it reads.)
5. **Review scoring formula.** "+12.4% impact" can be defined as:
   (a) what fraction of $-weight this single decision unblocks, or
   (b) the same divided by the *time-to-decide* (how clear-cut the
   decision is). Option (b) prioritises easy wins; option (a) is
   honest. I'd start with (a).
6. **Snooze duration.** Default 30 days for an orphan transfer? Or
   should it be configurable per-decision (with quick chips for
   7/30/90 days)?
