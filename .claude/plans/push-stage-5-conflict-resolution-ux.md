# Push Stage 5 — Conflict review/resolution UX

> See [push-overview.md](./push-overview.md) for the rollout context. Begin only after [Stage 4](./push-stage-4-per-field-conflict-detection.md) has produced enough real conflict rows in production to justify the work.

## Context

Stage 4 will populate `push_conflicts`. Stage 5 builds the human-in-the-loop tooling to triage that table: per row, pick "keep local / keep remote / skip" and act on the choice. Only worth building if Stage 4 actually produces conflict rows in production volume.

## Scope

- A reviewer (likely a TUI patterned on `/review-transfers`) that walks unresolved `push_conflicts` rows in batches.
- Persist each decision into the row (`resolution`, `resolved_at`).
- Next `push` run reads resolved rows and acts:
  - `resolution='local'` → push the local value (bypassing the conflict guard for this field).
  - `resolution='remote'` → overwrite the local DB to remote's value (creates a new `_transaction_changes` row with `reason='push-resolve'`); no PUT.
  - `resolution='skip'` → mark the row consumed, leave both sides as-is, do not retry until something changes.

## Files

- **new** `src/push/review.rs` — TUI logic.
- **new** `src/bin/push-review.rs` — entry point patterned on `src/bin/transfers.rs` `--review` mode.
- **new** `.claude/skills/review-push-conflicts/` — slash-command wrapper (mirrors `/review-transfers`).
- **edit** `src/push/mod.rs` — at top of run, scan `push_conflicts` for resolved rows; apply per the matrix above; clear them.

## Tests

24. `resolution='local'` → on next push run, that field pushes through despite still-conflicting remote.
25. `resolution='remote'` → local DB updated to remote value; `_transaction_changes` gets a `reason='push-resolve'` row; no PUT.
26. `resolution='skip'` → no action; row remains marked resolved so we don't ask again.
27. TUI snapshot: a fixture with 3 conflicts renders 3 reviewable cards.

## Open questions

1. **UI style.** ratatui TUI like `/review-transfers` (batches of 16, full keyboard)? Plain CLI prompts (one at a time)? Defer web entirely. Lean: ratatui, reusing whatever component lib `/review-transfers` already uses.
2. **Resolution options beyond local/remote/skip.** "Edit value directly" (free-text override)? "Ignore this field forever for this txn" (suppress future conflicts on this exact field+txn)? Lean: start with just the 3; add others only if Stage 4 data shows we need them.
3. **Apply timing.** When the user picks `local`, push immediately during the review, or persist the choice and let the next `push` run pick it up? Lean: defer to next run (keeps the reviewer pure-read-decide, no API calls from inside the TUI).
4. **`remote` resolution and audit.** When we accept remote → we mutate `transactions` locally. That mutation must go through `with_operation` with a new reason (`push-resolve`) so we have provenance. Confirm `pushed_at` is treated correctly on that history row (probably: set immediately, since accepting remote means there's nothing to push).
5. **`skip` semantics.** Does "skip" mean "decide later" (conflict persists, will be re-shown next time) or "skip forever for this conflict instance"? Two different UX needs; pick one explicitly. Lean: skip-forever for *this conflict row*, but if a NEW conflict arises later on the same field, surface it again.
6. **Concurrent edits.** User is mid-review when a `cargo run` pulls and either resolves the remote side (no longer conflicting) or makes it worse. Re-detect at TUI launch and either skip auto-resolved rows or update them in place.
7. **Bulk operations.** "Accept remote for all conflicts on field=`payee`" or "accept local for all conflicts from session=foo"? Probably out of scope for v1; revisit after Stage 5 ships.

## Gate

Manual user testing. No automated gate beyond Stage 4 having produced enough conflict rows to make Stage 5 worth shipping at all.
