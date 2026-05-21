# Push Stage 2 — Observation (no code)

> See [push-overview.md](./push-overview.md) for the rollout context. This stage gates Stages 3 and 4: real-world usage of [Stage 1](./push-stage-1-transfer-pair-mvp.md) determines what we build next.

## Context

Stage 1 ships the bluntest possible push. Stage 2 is the gate that decides what Stages 3 and 4 actually need to do. We do not want to design conflict detection (Stage 4) or expand to more fields (Stage 3) until we have seen Stage 1 behave in anger against real Pocketsmith data over real user activity.

## Scope

Use Stage 1 in production for a defined observation window. Read `push_log` (and the live API where needed). Produce a written debrief that explicitly answers: "what should Stage 3 and Stage 4 actually look like?"

## Activities

1. Run `push` daily (or after each `transfers --apply` session) over the observation window.
2. After each run, inspect `push_log`:
   - `SELECT outcome, COUNT(*) FROM push_log GROUP BY outcome` — baseline counts.
   - `SELECT * FROM push_log WHERE outcome='skipped_changed_upstream'` — every false-positive timestamp guard candidate.
3. For each `skipped_changed_upstream` row: manually GET the transaction from Pocketsmith (or look at `response_body` if we choose to also fetch on skips — see open question 3 below) and write down what *actually* changed remotely. We expect most to be unrelated fields (proves Stage 4 is worth doing).
4. For each `failed` row: read `error_message`, classify (network blip, server 5xx, validation 4xx, model mismatch).
5. Track any surprise: transfer pair where one side was deleted upstream, a PUT response that doesn't echo what we sent, etc.

## Deliverable

A short debrief committed at `plans/push-stage-2-debrief.md` (NEW — written after the observation window). Sections:

- Counts over the window (pushed / would_push / skipped_changed_upstream / deleted_upstream / failed).
- False-positive analysis: of N skips, M were on unrelated fields → Stage 4 priority.
- Surprises and what they imply for design.
- Explicit recommendation: do Stage 3 next, or Stage 4 next, or revise Stage 1 first?

## Open questions (resolve before starting Stage 2)

1. **Observation window length.** Calendar-fixed (1 week? 2 weeks?) or event-based ("until ≥ 20 pushed AND ≥ 5 skipped attempts")? Trade-off: too short → noisy conclusions; too long → bad UX (transfer pairs sit unpushed). Lean toward event-based with a hard cap of 2 weeks.
2. **Gate-to-proceed criteria.** What outcome distribution lets us move on cleanly? Proposal: `failed == 0` over the window, and we can articulate *why* every `skipped_changed_upstream` was skipped.
3. **Tooling.** Ad-hoc SQL only, or build a tiny `push-report` binary that summarises `push_log`? Lean ad-hoc to start; promote to a binary only if we find ourselves running the same three queries every day.
4. **Re-ordering Stages 3 vs 4.** If observation shows >50% of skips are false-positives on unrelated fields, the right move may be Stage 4 (better guard) before Stage 3 (more fields), so we don't ship a new fields' writes that immediately get false-blocked. Decide this in the debrief, not before.
5. **Pull-side surprises.** What if `pushed=N` produces an unexpected diff on the next `cargo run` pull (server overwrites our category, normalises something on its end)? Do we just record that, or add a Stage-2.5 task to study it before Stage 3? Capture as a follow-up if it happens; don't pre-design.

## Gate to Stage 3 / 4

Debrief committed; explicit recommendation on next-stage ordering; user agreement.
