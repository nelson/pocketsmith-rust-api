# Push rollout — overview

## Context

Local writes to `transactions` (from `normalise` and `transfers --apply`) currently never reach Pocketsmith. A previous attempt on branch `feature/writeback` (33 commits, ~960-line `src/writeback/mod.rs`, all 6 fields + per-field conflict detection + `push_conflicts` table) was built ahead of need. It will be archived as research, **not merged**. This plan starts fresh from `master`, in deliberate stages, each one small enough to use in anger and learn from before designing the next. Stages 2 and 4 in particular are gates: real-world observation determines what Stage 3 / Stage 5 look like.

Guiding rule: **never silently overwrite upstream**. Every stage refuses to write when upstream may have changed; the early stages use a blunt timestamp guard, later stages refine.

## Branch hygiene (before any code)

- `git tag archive/feature-writeback feature/writeback` then `git push origin archive/feature-writeback` so the research isn't lost.
- Develop on `claude/pocketsmith-writeback-kh025`. It currently tracks `master` (`4802656`); no reset needed at the time of writing.
- Do not cherry-pick from `feature/writeback`. Re-derive what's needed; reference the archive only for ideas.

## Stage list

| Stage | Name | Status | Plan |
|-------|------|--------|------|
| 1 | `is_transfer` + `category_id` push for confirmed transfer pairs | **done** (commit `6481871`) | (plan archived in git history) |
| 2 | Observation | **done** | [debrief](./push-stage-2-debrief.md) |
| 3 | Expand to `payee` (normalise) and remaining tracked fields | **done** (commit `89aab72`) | (plan archived in git history) |
| 4 | Replace timestamp guard with per-field conflict detection + conflicts table | future | [push-stage-4-per-field-conflict-detection.md](./push-stage-4-per-field-conflict-detection.md) |
| 5 | Conflict review/resolution UX | future | [push-stage-5-conflict-resolution-ux.md](./push-stage-5-conflict-resolution-ux.md) |

Stages 1-3 are implemented and live on `master`. Stage 4 is the next
actionable push work; the [Stage 2 debrief](./push-stage-2-debrief.md)
informs its design. Stage 5 only begins after Stage 4 has produced
real conflict rows.
