---
name: normalise-loop
description: Iterative normalisation improvement loop with metric-driven convergence
---

# Normalise Loop

Run an iterative improvement loop on the payee normalisation pipeline. Inspired by ResearcherSkill and autoresearch patterns.

## Setup (run once, at the start)

```bash
# 1. Create lab directory
mkdir -p lab

# 2. Measure baseline
python3 -m tools.normalise.measure --db pocketsmith.db --baseline \
  --disambiguation tools/normalise/disambiguation.yaml --output lab/baseline.json

# 3. Initialise iterations log
echo "iteration\tscore\tdelta\thypothesis\toutcome" > lab/iterations.tsv
```

## Each Iteration

### Phase 1: Analyse
- Read `lab/iterations.tsv` for history
- Read latest `lab/iteration_NNN/metrics.json` for sub-score breakdown
- Identify the weakest sub-score and specific failing cases
- Read `lab/iteration_NNN/diff_sample.json` for examples

### Phase 2: Hypothesise
- Choose ONE specific improvement (one stage, one type of rule)
- Write hypothesis to `lab/iteration_NNN/notes.md`
- Examples:
  - "Adding DOORDASH* to stage1 prefixes will improve S_noise by ~0.5"
  - "Adding PETERSHAM expansion to stage3 will fix 3 truncations"
  - "Adding YO-CHI identity mapping will collapse 5 duplicate payees"

### Phase 3: Apply
```bash
# Always start from fresh clone
cp pocketsmith.db lab/working.db

# Apply current rules
python3 -m tools.normalise.pipeline --db lab/working.db --rules tools/normalise/rules \
  --reason "normalise-vN"
```

### Phase 4: Measure
```bash
# Score
python3 -m tools.normalise.measure --db pocketsmith.db --rules tools/normalise/rules \
  --disambiguation tools/normalise/disambiguation.yaml \
  --output lab/iteration_NNN/metrics.json

# Diff sample
python3 -m tools.normalise.pipeline --db pocketsmith.db --rules tools/normalise/rules \
  --diff-sample 50 > lab/iteration_NNN/diff_sample.json
```

### Phase 5: Decide
- **Keep**: Score improved → commit rule changes, continue
- **Discard**: Score regressed → revert rule changes via git
- **Note**: Interesting but not better → log insight, try different approach

### Phase 6: Log
Append to `lab/iterations.tsv`:
```
NNN\t{score}\t{delta}\t{hypothesis}\t{outcome}
```

### Phase 7: Check stopping criteria
Stop when:
1. S_disambig = 100 AND Q >= 80
2. OR max 20 iterations reached

If stopped: apply final rules to `pocketsmith.db` (not the working copy).

## Discipline rules

1. **Commit before running** — git commit rule changes before applying
2. **Measure after** — always score after applying
3. **Log every result** — even failed experiments teach something
4. **Revert on discard** — don't accumulate bad changes
5. **One change per iteration** — atomic experiments only
6. **Never modify measure.py** — the metric is the constant
7. **S_disambig first** — correctness before optimisation

## Final application

When satisfied with the rules, apply to the real database:
```bash
python3 -m tools.normalise.pipeline --db pocketsmith.db --rules tools/normalise/rules \
  --reason "normalise-final"
```

Verify change tracking:
```bash
sqlite3 pocketsmith.db "SELECT COUNT(*) FROM _transactions_history WHERE reason = 'normalise-final' AND _mask & 1 = 1"
```
