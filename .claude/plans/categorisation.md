# Plan: Categorisation — the final pipeline stage

> Status: **drafting — gathering decisions.** CLI first, GUI second
> (same two-phase shape as the editable-rules arc: headless `rule` CLI →
> `editor-gui-framework`). Same testing strategy (hermetic unit + medium
> integration + narrow E2E; red-green; `warnings = "deny"`).

## Context

The app cleans transactions in stages: transfer detection → payee
normalisation (8 editable rule stages, DB-backed) → push to Pocketsmith.
**Categorisation is the missing final stage**: assign each transaction a
Pocketsmith `category_id` and a structured set of `labels`.

Grounding facts (verified against the live DB + code):
- **Categories are the user's own, synced from Pocketsmith** into the
  `categories` table. Currently **13, effectively flat** (12 roll-up
  parents prefixed `_`, plus `Eating Out`):
  `_Transfer, _Dining, _Income, _Mortgage, _Bills, _Education,
  _Groceries, _Transport, _Giving, _Household, _Shopping, _Holidays,
  Eating Out`.
- **~10,269 of 22,241 transactions are uncategorised**; 2,798 distinct
  merchant payees already exist in `payee_normalisations` (class=merchant).
- `transactions` already has `category_id` and `labels` (JSON array text);
  both are locally-writable and **already pushed** (push masks bit 2 =
  category, bit 3 = labels; `labels_for_put` CSV-encodes for the API).
- Normalisation already extracts, per merchant payee, an `entity_name`
  (canonical merchant) + `location` + `region` into
  `payee_normalisations.features_json`. **These are exactly the inputs a
  Google Places lookup needs.**

So categorisation slots in cleanly: it consumes a confirmed merchant
normalisation (entity + location), asks Google Places "what kind of place
is this?", maps the answer to one of the user's categories, and proposes a
category + labels — reviewed/confirmed/applied with the same
scan→confirm→apply paradigm as normalise.

## Scope (this arc)

- **Merchants only** to start (persons/employers/banking-ops are not
  places). A non-merchant payee is simply skipped by the categoriser.
- Primary goal: propose a **category_id**.
- Secondary goal: propose **labels** from a controlled, hierarchical
  vocabulary (no free-form proliferation).
- **Google Places responses cached** in SQLite so re-runs never re-hit
  the API.
- A **Google-Places-type → category** mapping that *I* author and seed
  (you don't maintain it); editable later like the other rule stages.
- CLI first; GUI as a follow-up stage.

## Proposed architecture

### 1. Google Places lookup + cache
- New module `src/categorise/places.rs`: a thin client over the **Places
  API (New) `places:searchText`** endpoint. Query = `"{entity_name}
  {location} {region}"`; request only the cheap fields we need
  (`places.id`, `places.displayName`, `places.primaryType`,
  `places.types`, `places.formattedAddress`). Mirrors `client.rs` style
  (blocking reqwest, env key `GOOGLE_PLACES_API_KEY`).
- New cache table `place_lookups`: keyed by the **normalised query**
  (so the 2,798 merchants, not 22k txns, drive at most one call each).
  Stores the raw JSON response + the extracted `primary_type` + `types`
  + `place_id`. A lookup checks the cache first; only a miss hits the API.
  TODO: exact columns in §schema.
- The categoriser **never calls the API implicitly** during a plain list
  render — only an explicit `categorise scan`/lookup does (mirrors the
  `rule_impact` "compute only on scan" discipline).

### 2. The Google-type → category mapping (HARDCODED in Rust)
> **Feedback #1:** Google place types are a limited, relatively static
> set, so the mapping is a **hardcoded Rust table**, not a DB table. No
> `rule_categories` table, no `rules/categories.sql`, no `Stage::Categories`.
- A single `const`/`static` data structure in `src/categorise/map.rs` is
  the **one source of truth** for the whole taxonomy, laid out so it can
  be reviewed at a glance (Feedback #4). Shape:
  ```rust
  // domain → category title + the leaves under it; each leaf lists the
  // Google place types that collapse into it.
  struct Domain { key: &str, category_title: &str, leaves: &[Leaf] }
  struct Leaf   { key: &str, place_types: &[&str] }
  static TAXONOMY: &[Domain] = &[ /* dining, groceries, transport, … */ ];
  ```
- `category_title` is matched to the user's `categories.title` at runtime
  to resolve a `category_id` (so the table isn't tied to one account's
  numeric ids). A place type not present in `TAXONOMY` → unmapped
  (leave uncategorised + flag for review).
- Because it's `const`, the mapping is covered by ordinary unit tests and
  reviewed in a single file — no seed/dump lifecycle to keep in lockstep.

### 3. Label / tag hierarchy strategy  → see "Open decisions" §B
(Proposed below; needs your sign-off before I bake it in.)

### 4. scan → confirm → apply workflow (mirror normalise)

### 4.0 Eligibility gate (added per UAT) — `src/categorise/gate.rs`
Categorise only runs for payees whose normalisation is settled, so we
never spend a Places lookup on a string about to change. A payee is
eligible iff **applied OR confirmed, minus pending**:
  * **applied** — a `normalise-apply` (or legacy `normalisation`) committed
    payee write exists in `_transaction_changes` (the durable signal, since
    apply drains the staging row);
  * **confirmed** — the payee has a confirmed (status=1) staging row;
  * **minus pending** — exclude any payee with a pending (status=0) row.
On the current DB this is ~5 confirmed merchants (most normalise work is
still pending), exactly the intended dependency: categorise waits for the
normalise confirm+apply queue.

### 4.1 Equivalence guard (added per UAT)
Mirrors normalise's skip-no-change: if a merchant's transactions already
carry the proposed category+labels, the scan skips it (`skipped_no_change`)
instead of re-staging — so re-scanning applied merchants is a no-op.

### 4.2 Lookup errors are not cached
Transport/API errors are returned but **not** persisted to `place_lookups`
(only `ok`/`no_result` are cached), so a transient failure retries on the
next scan. Surfaced as a separate `skipped_error` stat.
- New staging table `category_proposals` (parallels
  `payee_normalisations`): one row per distinct merchant key, with
  proposed `category_id`, proposed `labels`, the source `place_type`,
  status (pending/confirmed/rejected), txn_count.
- `categorise scan`: for each confirmed merchant normalisation, look up
  the place (cache-first), map type → category + labels, upsert a pending
  proposal. Same 4-row policy table as `normalise scan`.
- `categorise apply`: write confirmed `category_id` + `labels` to all
  matching `transactions`, under `with_operation("categorise-apply", …)`;
  the existing change-triggers + push pick them up.

### 5. CLI (`categorise` bin or subcommands) — phase 1
Mirror the `rule` / `normalise` CLI conventions (hand-rolled args, text +
`--json`, dry-run by default, `--apply` to commit, exit codes):
```
categorise scan                # cache-first Places lookups, stage pending proposals
categorise list [--status …]   # show proposals (text / --json)
categorise confirm <merchant>  # mark a proposal confirmed
categorise reject  <merchant>
categorise apply               # write confirmed category_id + labels to transactions
categorise lookup "<query>"     # ad-hoc Places probe (uses + fills the cache)
```
No network on `list`/`apply`; only `scan`/`lookup` may hit the API (and
only on a cache miss).

## Implementation checklist
- [x] Step 1 — schema: `place_lookups` + `category_proposals` in
      `db/schema.rs` (+ `category_proposals_updated_at` trigger);
      `src/db/{place_lookups,category_proposals}.rs` helpers + tests.
- [x] Step 2 — `src/categorise/map.rs` hardcoded `static TAXONOMY` (single
      reviewable source) + `map_place_type`/`map_types`/`resolve_category`
      + uniqueness-invariant tests.
- [x] Step 3 — `src/categorise/places.rs`: Places (New) `searchText` client
      behind the `PlacesClient` seam; cache-first `lookup`; canned-JSON tests.
- [x] Step 4 — `src/categorise/propose.rs`: pure lookup→(category_id, leaf
      labels) builder; 8 unit cases incl. unmapped + multi-type precedence.
- [x] Step 5 — `src/categorise/scan.rs`: pipeline-aggregated merchants →
      cache-first lookup → propose → upsert pending; `ScanStats`; idempotent.
- [x] Step 6 — `src/categorise/apply.rs`: write confirmed category+labels
      under `with_operation("categorise-apply")`; push mask asserted.
- [x] Step 7 — `src/bin/categorise.rs` CLI (scan/list/confirm/reject/apply/
      lookup, text + `--json`) + `Cargo.toml` bin; in-crate e2e test.
- [x] Step 8 — GUI: dedicated **Categorise tab** (`src/bin/serve/categorise/*`)
      reusing the three-pane shell; proposal queue (list→detail→confirm/
      reject/skip/undo + apply); routes + tab-bar entry + smoke test.

## Verification
- `cargo test` — all unit + integration green; `warnings = "deny"` clean.
- Manual: obtain key (§10) → `categorise lookup "Woolworths Strathfield"`
  returns a type + populates the cache → `categorise scan` stages pendings
  → `categorise list` → `categorise confirm …` → `categorise apply` sets
  `category_id`+`labels` on the matching txns → `push` sends them upstream.
- Re-run `categorise scan`: zero new API calls (all cache hits).

### 6. GUI — phase 2
**Categorise is its own top-level tab** (Feedback #2 — not folded into
Review), reusing the three-pane shell. Deferred until the CLI is proven,
exactly like the editable-rules arc. The mapping taxonomy is code, not
user-editable, so the tab is a proposal queue (list → detail → confirm/
reject), not a rule editor.

## Files (anticipated)
- new `src/categorise/{mod,places,scan,apply,map}.rs`
- new `src/db/{place_lookups,category_proposals}.rs`
- new `src/bin/categorise/*` (CLI)
- edit `src/db/schema.rs` (2 new tables + trigger), `src/db/mod.rs`
  (module wiring), `src/lib.rs` (expose `categorise`), `Cargo.toml`
  (new bin)
- phase 2 (own tab): `src/bin/serve/categorise/*`, `css.rs`, `tab.rs`

## Reuse
- `db::with_operation`, `db::open_app_db` — operation framing + seeding.
- `payee_normalisations` (entity_name/location/region in features_json) —
  the Places query inputs; categorisation only runs on confirmed merchants.
- scan/confirm/apply paradigm + `ScanStats`/`ApplyStats` (`normalise/scan.rs`,
  `normalise/apply.rs`, `src/review.rs`).
- rule seed/dump/edit lifecycle (`rules/mod.rs`: `load_into_db`,
  `dump_stage`, `schedule_dump`, `Stage`) for the editable mapping table.
- push already handles `category_id` + `labels` — no push changes needed.
- `client.rs` patterns for the new Places HTTP client.

## Decisions locked (from planning Q&A)
1. **Google Places API (New)** — POST `places:searchText` with a field
   mask; key in `GOOGLE_PLACES_API_KEY`. User does not have a key yet —
   plan ends with a Google Cloud console walkthrough (§10).
2. **Label hierarchy = controlled `domain/leaf` vocabulary** (§B below).
3. **Key on the confirmed merchant; always review.** Every proposal
   starts `pending`; the user confirms before apply (identical to the
   normalise scan→confirm→apply flow). No auto-confirm.
4. **Push upstream** — confirmed `category_id` + `labels` become locally
   dirty and the existing `push` run sends them upstream. No push code
   changes (it already masks category=bit2, labels=bit3).

### B. Label/tag hierarchy strategy (locked + Feedback #3/#4)
- **Controlled vocabulary, two-level hierarchy that lives in code.** The
  `TAXONOMY` structure (§2) *is* the hierarchy: each domain owns a set of
  leaves; each leaf collapses several Google place types. This is the
  single reviewable source (Feedback #4).
- **The label written to Pocketsmith is the LEAF ONLY** (Feedback #3) —
  e.g. `cafe`, `supermarket`, `fuel`, `clothing` — never the `domain/leaf`
  path. The domain is implied by the category and exists only in the code
  taxonomy for grouping/review; it is not written to the label string.
- **No label is ever emitted outside this vocabulary** — that prevents
  proliferation. Leaf keys are globally unique across domains so a bare
  leaf is unambiguous.
- Net effect: every categorised merchant gets **one category + one leaf
  label**, both drawn from the fixed code taxonomy.

## 7. Schema (new tables)

```sql
-- Cache of Google Places lookups. Keyed by the normalised query so the
-- ~2,798 merchants drive at most one API call each, ever. A scan reads
-- this first and only hits the network on a miss.
CREATE TABLE IF NOT EXISTS place_lookups (
    query         TEXT PRIMARY KEY,   -- normalised "{entity} {location} {region}"
    place_id      TEXT,               -- Google place id (NULL = no result)
    display_name  TEXT,
    primary_type  TEXT,               -- e.g. 'cafe'
    types_json    TEXT NOT NULL,      -- full types array as JSON
    response_json TEXT NOT NULL,      -- raw API body (audit / re-derive)
    fetched_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    status        TEXT NOT NULL DEFAULT 'ok'  -- 'ok' | 'no_result' | 'error'
);

-- Staging table for proposals, parallel to payee_normalisations. One row
-- per distinct confirmed-merchant key. The type→category/label mapping
-- itself is HARDCODED in src/categorise/map.rs (Feedback #1) — no
-- rule_categories table.
CREATE TABLE IF NOT EXISTS category_proposals (
    merchant_key      TEXT PRIMARY KEY,   -- the normalised query / merchant identity
    proposed_category INTEGER,            -- category_id (resolved), NULL = unmapped
    proposed_labels   TEXT,               -- JSON array of leaf labels, controlled vocab only
    place_type        TEXT,               -- the type that drove the mapping
    txn_count         INTEGER NOT NULL,
    status            INTEGER NOT NULL DEFAULT 0 REFERENCES statuses(id),
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```
- `category_proposals` gets an `updated_at` trigger like the other staging
  tables. Only **two** new tables (no `rule_categories`).
- Exact `place_lookups` key normalisation + the `merchant_key` definition
  finalised in step 1 of the checklist.

## 8. The hardcoded taxonomy (`src/categorise/map.rs`, Feedback #1/#4)

One `static TAXONOMY` is the single reviewable source. Domain → category
title → leaves → Google place types. **Only the leaf is written as the
label** (Feedback #3). Initial contents:

| domain | category_title | leaf (label written) | Google place_types |
|--------|----------------|----------------------|--------------------|
| dining | Eating Out | `restaurant` | restaurant, meal_takeaway, meal_delivery |
| dining | Eating Out | `cafe` | cafe, coffee_shop, bakery |
| dining | Eating Out | `bar` | bar, pub, night_club |
| groceries | _Groceries | `supermarket` | supermarket, grocery_store, convenience_store |
| transport | _Transport | `fuel` | gas_station |
| transport | _Transport | `transit` | parking, transit_station, train_station, bus_station, taxi_stand, subway_station |
| shopping | _Shopping | `clothing` | clothing_store, shoe_store, jewelry_store |
| shopping | _Shopping | `retail` | department_store, shopping_mall, store, supermarket? (no) |
| household | _Household | `home_goods` | electronics_store, hardware_store, furniture_store, home_goods_store |
| bills | _Bills | `health` | pharmacy, drugstore, hospital, doctor, dentist |
| bills | _Bills | `financial` | bank, atm, insurance_agency, accounting |
| education | _Education | `education` | school, university, library, book_store |
| holidays | _Holidays | `travel` | lodging, hotel, travel_agency, airport |
| giving | _Giving | `charity` | church, place_of_worship |
| (no match) | — leave uncategorised, flag for review | — | — |

Leaf keys are unique across domains so the bare label is unambiguous.
Final exact rows land in code; this table is the review surface in the
plan, the `static TAXONOMY` is the review surface in the tree.

## 9. Testing strategy (unchanged project conventions)
- **Unit (broad, hermetic):** the type→category/label mapper as a pure fn
  over the `static TAXONOMY` (≥8 cases incl. unmapped fallback + multi-type
  precedence + leaf-uniqueness invariant); the Places response parser over canned JSON bodies (no
  network); `merchant_key` normalisation; scan policy table (4 rows,
  mirroring `normalise/scan.rs`); apply writes category+labels and only
  touches changed rows.
- **Integration (medium):** seed merchants + a cached `place_lookups`
  row → `categorise scan` proposes pending → confirm → `categorise apply`
  writes `transactions.category_id` + `labels` under
  `with_operation("categorise-apply")` → the change-trigger fires →
  a subsequent push would send it (assert the dirty mask).
- **Network is never hit in tests** — the cache table is pre-seeded; a
  single thin seam (`PlacesClient` trait or injected fn) lets tests supply
  canned responses, matching how the codebase keeps HTTP out of tests.
- Red-green commits; `warnings = "deny"`; net new code warning-clean.

## 10. Getting a Google Places API key (walkthrough — for after planning)
Step-by-step in the Google Cloud console:
1. Create/select a project at console.cloud.google.com.
2. Enable billing (Places API New requires a billing account; there's a
   monthly free tier — caching keeps us well inside it).
3. APIs & Services → Library → enable **"Places API (New)"**.
4. APIs & Services → Credentials → Create credentials → **API key**.
5. Restrict the key: API restriction → Places API (New); optionally an
   application restriction (none needed for a server-side blocking client).
6. Put it in `.env` as `GOOGLE_PLACES_API_KEY=...` (gitignored).
7. Smoke-test with a single `searchText` curl before the first scan.
(Expanded with exact screen names when we reach implementation.)
