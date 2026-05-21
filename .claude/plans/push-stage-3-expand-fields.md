# Push Stage 3 — Expand to remaining locally-mutated fields

> See [push-overview.md](./push-overview.md) for the rollout context. Do not begin this stage until the [Stage 2](./push-stage-2-observation.md) debrief is committed.

## Context

[Stage 1](./push-stage-1-transfer-pair-mvp.md) only pushes `is_transfer` + `category_id` from `transfers --apply`. The other place we write locally today is `normalise`, which mutates `payee` (and maybe other fields — verify). Stage 3 generalises Stage 1's pending-pull-PUT-mark loop to handle every locally-mutated field, still gated by the Stage 1 timestamp guard.

## Scope

- Drop Stage 1's `(mask & 16) != 0` and `reason='transfers'` filters from the pending query.
- Build the `TransactionUpdate` body from the *union* of dirty bits across the txn's unpushed history rows (so two history rows — one from normalise, one from transfers — get folded into one PUT).
- All Stage 1 safety still applies: timestamp guard, `pushed_at` marking, `push_log` row per attempt.

## Pending query (Stage 3 form, replaces Stage 1's)

```sql
SELECT DISTINCT c.transaction_id
FROM _transaction_changes c
JOIN _operations o ON c.operation_id = o.id
WHERE c.mask != 0
  AND o.reason NOT IN ('sync','push')
  AND c.pushed_at IS NULL
  AND EXISTS (SELECT 1 FROM transactions t WHERE t.id = c.transaction_id);
```

## TransactionUpdate construction

Per txn: union the masks of all matching unpushed history rows. For each dirty bit, pull the *current* `transactions.<field>` value (we always send the latest local truth, not the history-row value). Set the corresponding `Option` on `TransactionUpdate`; leave other `Option`s `None` so they're omitted from the JSON body (`skip_serializing_if = "Option::is_none"`).

## Files

- **edit** `src/push/mod.rs` — generalise pending query, generalise body builder, generalise mark-as-pushed (covers all involved `_transaction_changes` rows).
- Possibly **edit** `src/db/schema.rs` — only if we discover during Stage 2 that we need a new bit on the mask for a field we don't yet track. Otherwise no schema change.

## Tests (additions on top of the Stage 1 test list)

14. Pending query picks up `reason='normalisation'` rows.
15. Pending query ignores `reason='sync'` and `reason='push'`.
16. Two unpushed history rows on the same txn (one normalisation, one transfers) → single PUT with both fields set.
17. After a successful Stage-3 PUT, all involved `_transaction_changes` rows have `pushed_at` set.
18. Field-specific serialisation: labels JSON-array → CSV on wire (verify against `models::TransactionUpdate::serialize` snapshot).

## Open questions

1. **Expansion order.** All fields at once, or incrementally (payee first, then note/labels/memo)? Smaller increments give faster feedback. Probably: payee first as its own commit, remaining fields second.
2. **Combined PUTs across reasons.** When a txn has dirty bits from BOTH `normalisation` AND `transfers`, do we combine in one PUT? Default yes (matches the Stage 1 logic of "PUT the local truth in one shot"), but it does couple two independent writer-systems' fates together — if the combined PUT fails, both source-system commits are still un-pushed. Acceptable.
3. **Labels JSON↔CSV transform.** Local stores `labels` as a JSON array (verify this — may actually be a comma-separated string in SQLite); wire format is CSV. Does `models::TransactionUpdate`'s serializer already do this correctly? Verify with a snapshot test before assuming.
4. **Which fields are actually mutated locally today?** Grep for `with_operation` callers / `UPDATE transactions` to enumerate. Today: `transfers` (`is_transfer` + `category_id`), `normalise` (`payee`, and possibly more — confirm). Anything else?
5. **Unsetting `is_transfer`.** Today `transfers --apply` only sets it true. If we ever unset (a "this isn't really a transfer" review action), do we push `is_transfer: Some(false)` or omit? Probably push `Some(false)` so remote learns. Confirm at Stage 3 design time.
6. **Settle window.** Stage 1 has none — for transfer pairs the user has already manually confirmed. For Stage 3 (especially normalisation, which is bulk-automated), do we want to delay PUTs by N days so the user has a chance to undo? Decide based on Stage 2 observation of how often we'd regret an auto-push.

## Gate to Stage 4

Tests green; manual smoke on a real DB with both reasons present; a Stage 2-style mini-observation (a few days) shows no new regressions.
