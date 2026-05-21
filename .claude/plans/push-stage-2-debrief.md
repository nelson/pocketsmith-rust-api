# Push Stage 2 — Observation debrief

> Closes the gate set in [push-stage-2-observation.md](./push-stage-2-observation.md). Recommendation at the bottom.

## Window

- **Start:** 2026-05-21T02:42:47Z (first `push` invocation after Stage 1 landed).
- **End:** 2026-05-21T12:06:32Z (most recent `push` invocation as of writing).
- **Calendar duration:** ~9.5 hours, single calendar day.
- **Why short:** event-based exit hit immediately. Stage 1 + the `--annotate-existing` follow-on produced enough push activity to answer every Stage 2 question on day 1; there was no value in stretching the window further with no new code in flight.

## Outcome counts

```
SELECT outcome, COUNT(*) FROM push_log GROUP BY outcome;
```

| outcome                    | n  |
|----------------------------|----|
| `pushed`                   | 26 |
| `would_push`               | 26 |
| `skipped_changed_upstream` |  0 |
| `deleted_upstream`         |  0 |
| `failed`                   |  0 |

26 real PUTs across 20 distinct transactions. The 6 txns with two real pushes are the original transfer pairs from session 1, re-pushed in session 3 after `transfers --annotate-existing` backfilled `[paired:<id>]` memos on them.

Three push sessions in the window:

| time (UTC) | pushed | trigger                                      |
|------------|--------|----------------------------------------------|
| 02:42–02:43|   6    | first `transfers --apply` batch              |
| 12:02      |  14    | second `transfers --apply` batch             |
| 12:06      |   6    | `transfers --annotate-existing` memo backfill |

## False-positive analysis (`skipped_changed_upstream`)

Zero skips, so nothing to classify. The timestamp guard never fired in this window. This is consistent with the operating pattern: every push session ran within minutes of a `transfers --apply` (or `--annotate-existing`) preceded by a fresh `sync`, so local and remote `updated_at` had no time to diverge from external activity.

We therefore have **no empirical signal yet** on Stage 4's central premise (that the timestamp guard produces a high false-positive rate). We will only learn that from a window that includes either (a) external Pocketsmith activity (mobile app edits, rules-engine changes) between sync and push, or (b) a longer gap between `transfers --apply` and `push`.

## Failed / deleted classification

Zero of each. No network, server, validation, or model-mismatch errors observed.

## Surprises and what they imply

1. **Pre-fix `pushed_at` over-stamping on sync rows.** The first push session (02:42 UTC) ran on commit `2101619` (Stage 1 MVP), which had a bug where the `pushed_at` UPDATE didn't filter on `_operations.reason = 'transfers'`. As a result, the mask=63 sync-create row for each pushed txn (e.g. `_transaction_changes.id=99`, `transaction_id=1820486295`) was incorrectly stamped at 02:42:57Z. The bug was fixed in `d787865` ("push: respect transactions invariant, fix mask=63 marking bug") at 03:19 UTC, before the second and third sessions, which behave correctly.
   - *Impact on production data:* harmless retrospectively. The sync rows that got stamped were the create-records for those txns, and there was nothing to push from them anyway. No data loss, no double-PUT.
   - *Implication for Stage 3:* the test added in `src/push/tests.rs` ("sync rows (incl. the mask=63 create marker) must never be stamped pushed_at") is now load-bearing — it must keep passing as we generalise the pending-query and the stamping query.

2. **Server PUT response echoes our payload faithfully.** All 26 `pushed` `response_body` rows have `is_transfer=true`, the right `category.id`, and the memo we sent. Pocketsmith does not appear to silently coerce or normalise these fields on PUT. No need for the "what if PUT response doesn't echo what we sent" surprise hook from the plan.

3. **Local `updated_at` strategy is working as designed.** Push deliberately does not bump `transactions.updated_at` after a successful PUT. Concrete evidence from txn `1820486295`:
   - Session 1 PUT at 02:42:57Z. Server's post-PUT `updated_at` was `2026-05-21T02:42:57Z` (in `push_log.response_body`). Local was *not* bumped.
   - Sync at 02:47 UTC ran with `updated_since` = pre-push timestamp, refetched the row, and wrote the server's new `updated_at` into local.
   - Session 3 GET at 12:06:14Z saw remote `updated_at = 2026-05-21T02:42:57Z`, local `updated_at = 2026-05-21T02:42:57Z` — match, guard passes, second PUT proceeds. This is exactly the loop documented in `src/push.rs`.

4. **No transfer pair where one side was deleted upstream** during the window. Stage 4's "deleted_upstream during a pair PUT" edge case is theoretical only so far.

## Open questions — resolved

1. **Observation window length.** Resolved as event-based: window closes when `failed == 0` over a window that contains a non-trivial number of pushes and we can articulate every skip. Hard cap was unused; window closed in <1 day.
2. **Gate-to-proceed criteria.** `failed == 0`: ✅ (0/52 attempts). Every skip explained: ✅ vacuously (0 skips).
3. **Tooling.** Ad-hoc SQL was sufficient. Not promoting to a `push-report` binary; if Stage 3 generates daily volume worth summarising, revisit then.
4. **Re-ordering Stages 3 vs 4.** No data supports prioritising Stage 4 over Stage 3 — false-positive rate is unmeasured (0 skips). Default ordering stands: **Stage 3 next.**
5. **Pull-side surprises.** None observed. Server's post-PUT echo matches local intent; the next sync absorbs the bumped `updated_at` cleanly. No Stage-2.5 follow-up needed.

## Recommendation

**Proceed to Stage 3 (expand to remaining locally-mutated fields).** Specifically:

- Stage 1's safety harness behaved correctly across 26 real PUTs once the `mask=63` over-stamping bug was fixed. Generalising the pending query and `TransactionUpdate` body builder is the right next step.
- The `normalise` binary writes `payee` under `with_operation("normalisation", ...)` and is currently invisible to `push`. Expanding the pending query to `o.reason NOT IN ('sync','push')` is the highest-leverage change.
- Stage 4 (per-field conflict detection) can wait until we have a window with non-zero `skipped_changed_upstream` — i.e. either organic external Pocketsmith activity, or a deliberate "let edits sit for N days before pushing" experiment. Building Stage 4 now would be designing on zero data.

User agreement: …
