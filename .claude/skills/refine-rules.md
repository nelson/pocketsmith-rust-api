---
name: refine-rules
description: Make incremental improvements to normalisation rules based on metric feedback
---

# Refine Rules

Make ONE targeted improvement to the normalisation rules based on the latest metric scores.

## Process

1. **Read iteration history**:
   ```bash
   cat lab/iterations.tsv
   cat lab/iteration_NNN/metrics.json  # latest iteration
   ```

2. **Identify the weakest sub-score**:
   - `S_disambig`: Check `disambig_failures` — fix specific test cases
   - `S_dedup`: Look at payees that should collapse but don't — add identity mappings
   - `S_entity`: Find `generic` type payees — add classification patterns
   - `S_noise`: Find payees still containing card numbers, dates, receipt numbers
   - `S_truncation`: Find truncated words not yet in the dictionary

3. **Formulate a hypothesis**: "Adding X to stage Y rules will improve Z by ~N points"

4. **Edit ONE rules file** (the most impactful change)

5. **Test on fresh clone**:
   ```bash
   cp pocketsmith.db lab/working.db
   python3 -m tools.normalise.pipeline --db lab/working.db --rules tools/normalise/rules --reason "normalise-vN" --dry-run
   ```

6. **Measure**:
   ```bash
   python3 -m tools.normalise.measure --db pocketsmith.db --rules tools/normalise/rules --disambiguation tools/normalise/disambiguation.yaml
   ```

7. **Decide**:
   - Score improved → **Keep** the rule change
   - Score regressed → **Revert** the rule change
   - Score unchanged but interesting → **Note** for later

8. **Log** to `lab/iterations.tsv` and save metrics to `lab/iteration_NNN/metrics.json`

## Rules for changes

- ONE change per iteration (atomic)
- Always target the weakest sub-score first
- S_disambig must be 100 before optimising other scores
- Never modify `tools/normalise/measure.py` or `disambiguation.yaml` test case logic
- New disambiguation test cases CAN be added if you discover new hard constraints
