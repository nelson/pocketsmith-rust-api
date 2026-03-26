# Iteration 036: Strip "LS " (Lightspeed) gateway prefix

## Hypothesis
"LS " is an abbreviation for Lightspeed POS gateway. Some transactions
start with "LIGHTSPEED*LS " (where LIGHTSPEED* is stripped but LS remains)
and others start directly with "LS ". Stripping this prefix will consolidate
~26 merchants with their non-LS-prefixed counterparts.

## Changes
- **stage1_rules.yaml**: Added `LS\s+` as a gateway prefix pattern;
  also updated LIGHTSPEED* pattern to optionally strip trailing "LS "

## Result
Q: 93.66 → 93.66 (+0.00, rounds to same)
S_dedup: 74.82 → 74.85
Unique payees: 4168 → 4162 (-6)
