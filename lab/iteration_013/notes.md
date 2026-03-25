# Iteration 013: Uppercase normalisation + IGNORECASE regexes

**Hypothesis**: Uppercasing payee at start of stage 1 + case-insensitive regex matching will collapse case-variant duplicates. Stage 5 title-case restores proper casing.

**Result**: Q +0.07 (92.87→92.94). 24 unique payees eliminated, 26 fewer long-tail payees, 23 fewer singletons. Case variants now collapse correctly.
