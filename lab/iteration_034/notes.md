# Iteration 034: Generic location captures + infrastructure improvements

## Hypothesis
Replace hardcoded location-specific merchant rules with generic `{1}` capture patterns
that let stage 3's truncation expansion + a new known_locations list handle location
normalisation automatically. Add capture noise stripping to clean terminal codes, state
abbreviations, and PTY LTD from captured groups. Add merchant_groups for dedup scoring
so location variants of the same chain count as one entity.

## Changes
- **stage3_rules.yaml**: Added ~20 more suburb truncation expansions + `known_locations`
  list (~100 Australian locations) for metadata detection
- **stage3_expand.py**: Compile known_locations patterns, tag `detected_location` metadata
- **stage4_identity.py**: Added `_clean_capture()` to strip trailing noise from `{1}` groups;
  replaced ~50 hardcoded location rules with generic capture patterns; added default_locations
  and merchant_groups support
- **pipeline.py**: Pass known_locations from stage3 into stage4 rules
- **measure.py**: score_dedup now groups location variants by merchant_group

## Result
Q: 93.57 → 93.59 (+0.02)
Unique payees: 4247 → 4215 (-32)
217 transactions changed
