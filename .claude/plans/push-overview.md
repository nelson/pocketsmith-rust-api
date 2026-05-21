# Push rollout — overview

## Context

Local writes to `transactions` (from `normalise` and `transfers --apply`) currently never reach Pocketsmith. A previous attempt on branch `feature/writeback` (33 commits, ~960-line `src/writeback/mod.rs`, all 6 fields + per-field conflict detection + `push_conflicts` table) was built ahead of need. It will be archived as research, **not merged**. This plan starts fresh from `master`, in deliberate stages, each one small enough to use in anger and learn from before designing the next. Stages 2 and 4 in particular are gates: real-world observation determines what Stage 3 / Stage 5 look like.

Guiding rule: **never silently overwrite upstream**. Every stage refuses to write when upstream may have changed; the early stages use a blunt timestamp guard, later stages refine.

## Branch hygiene (before any code)

- `git tag archive/feature-writeback feature/writeback` then `git push origin archive/feature-writeback` so the research isn't lost.
- Develop on `claude/pocketsmith-writeback-kh025`. It currently tracks `master` (`4802656`); no reset needed at the time of writing.
- Do not cherry-pick from `feature/writeback`. Re-derive what's needed; reference the archive only for ideas.

## Stage list

| Stage | Name | Code? | Gate | Plan |
|-------|------|-------|------|------|
| 1 | `is_transfer` + `category_id` push for confirmed transfer pairs | yes | passes own tests + manual smoke | [push-stage-1-transfer-pair-mvp.md](./push-stage-1-transfer-pair-mvp.md) |
| 2 | Observation | no — usage only | written debrief informs Stage 3 | [push-stage-2-observation.md](./push-stage-2-observation.md) ([debrief](./push-stage-2-debrief.md)) |
| 3 | Expand to `payee` (normalise) and remaining tracked fields | yes | tests + manual | [push-stage-3-expand-fields.md](./push-stage-3-expand-fields.md) |
| 4 | Replace timestamp guard with per-field conflict detection + conflicts table | yes | tests + manual | [push-stage-4-per-field-conflict-detection.md](./push-stage-4-per-field-conflict-detection.md) |
| 5 | Conflict review/resolution UX | yes | only after seeing real conflicts in Stage 4 | [push-stage-5-conflict-resolution-ux.md](./push-stage-5-conflict-resolution-ux.md) |

Only Stage 1 is fully specified. Stages 2–5 are real plans of *what we will do at that stage*, including the open questions we deliberately want to defer until we have data from the previous stage.
