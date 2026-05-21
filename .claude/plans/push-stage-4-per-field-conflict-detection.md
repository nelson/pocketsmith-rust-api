# Push Stage 4 — Per-field conflict detection

> See [push-overview.md](./push-overview.md) for the rollout context. Begin only after a Stage 2-style observation period on top of [Stage 3](./push-stage-3-expand-fields.md).

## Context

Stages 1–3 use a blunt guard: any change to `updated_at` on the remote → refuse all PUTs for that txn. [Stage 2](./push-stage-2-observation.md) observation will quantify how often this false-blocks (we change `payee`, remote changed `note` two days ago → blocked). Stage 4 replaces the blunt guard with a per-field check, and starts recording real conflicts.

## Scope

- Replace timestamp guard with per-field baseline-vs-remote comparison.
- Introduce `push_conflicts` table to record fields where local diverged from baseline AND remote diverged from baseline → genuine conflict.
- Each PUT either pushes all dirty fields (none conflict) or pushes a subset (some fields conflict, others don't — see open question 4).

## Baseline value

For a dirty field `F` on txn `T`, the baseline is `T.F`'s value at the moment we last pulled it from Pocketsmith — i.e. the most recent `_transaction_changes` row for `T` with `reason='sync'` AND mask bit for `F` set. We materialise this lazily during push (no new table).

## Decision matrix per dirty field

| `local == baseline?` | `remote == baseline?` | Action |
|----------------------|-----------------------|--------|
| no (we changed it)   | yes (remote unchanged) | Push |
| no (we changed it)   | no  (remote also changed) | **Conflict** — record in `push_conflicts`, don't push this field |
| yes (we matched baseline — shouldn't happen if dirty bit set) | — | Log; treat as Push (it's a no-op) |

## `push_conflicts` table

```sql
CREATE TABLE IF NOT EXISTS push_conflicts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    txn_id              INTEGER NOT NULL,
    field               TEXT NOT NULL,
    baseline_value      TEXT,
    local_value         TEXT,
    remote_value        TEXT,
    detected_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    resolution          TEXT,                    -- NULL | 'local' | 'remote' | 'skip'
    resolved_at         TEXT,
    UNIQUE(txn_id, field, detected_at)
);
CREATE INDEX IF NOT EXISTS idx_wb_conflicts_unresolved ON push_conflicts(resolution) WHERE resolution IS NULL;
```

Values stored as text JSON (so labels arrays, NULL category_id, etc. all round-trip cleanly).

## Files

- **edit** `src/push/mod.rs` — replace `timestamp_guard()` with `per_field_conflict_check()`; thread conflict rows into both `push_log` (outcome=`conflict_recorded`) and `push_conflicts`.
- **edit** `src/db/schema.rs` — add `push_conflicts`.
- **edit** `src/db/mod.rs` — `CREATE TABLE IF NOT EXISTS` is idempotent; no `ALTER TABLE` migration needed.

## Tests (additions)

19. Local changed payee, remote unchanged → push payee.
20. Local changed payee, remote ALSO changed payee → conflict recorded; no PUT; `push_conflicts` has one row.
21. Local changed `is_transfer`; remote changed `note` (not `is_transfer`) → push (no conflict, narrow guard works).
22. Mixed: local changed `payee`+`category`, remote changed `payee` → conflict on `payee`, push `category` alone in same PUT (if open question 4 = partial).
23. Re-run after conflict: same conflict not duplicated (UNIQUE constraint).

## Open questions

1. **Baseline source.** "Most recent `reason='sync'` history row with this bit set" vs "most recent successful push response (we stored `response_body` in `push_log`)". The former is correct for fields the user changes locally that we haven't pushed yet; the latter is correct *after* a push (the response is the new baseline). Probably: use the more recent of the two.
2. **`push_conflicts` granularity.** One row per (txn, field, detection event) with UNIQUE on (txn, field, detected_at)? Or upsert one row per (txn, field) that gets updated as new conflicts arise? Lean: append-only with UNIQUE (we want a history).
3. **Outcome taxonomy in `push_log`.** Keep `skipped_changed_upstream` as a Stage-1 holdover, or replace with `conflict_recorded` (Stage 4)? Probably introduce `conflict_recorded` and let `skipped_changed_upstream` go to zero naturally.
4. **Partial push vs all-or-nothing.** Txn has 3 dirty fields, 1 conflicts → push the 2 clean fields, record conflict on the 1? Or block the whole PUT until conflict is resolved? Partial is safer (less work lost), all-or-nothing is simpler (atomic). Lean partial; decide at Stage 4 design time after Stage 2 data.
5. **Surfacing conflicts.** Just print `conflicts: N` in push output? Or refuse to run when N > some threshold? Lean: always run, always print, never refuse.
6. **Stale conflicts.** If a conflict row is recorded today, then tomorrow the user pulls and the remote no longer diverges (admin reverted), do we auto-resolve the conflict row (mark `resolution='auto-stale'`)? Probably yes; design it into Stage 4 not Stage 5.

## Gate to Stage 5

Tests green; manual smoke; an observation period showing genuine conflicts being recorded (otherwise [Stage 5](./push-stage-5-conflict-resolution-ux.md) may be a no-op and we skip it).
