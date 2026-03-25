# Iteration 016: Stage 5 trailing noise (VISA, BANK, etc.) + state uppercase_exceptions

**Hypothesis**: Adding trailing VISA/BANK/DEBIT/CREDIT noise patterns and state abbreviation uppercase exceptions will strip residual noise and improve title casing.

**First attempt**: Including `\s+\d{4,6}$` caused S_disambig to drop to 90.91 — stripped Australian postcodes (4 digits) causing Woolworths location over-merge. Reverted and retried without it.

**Result**: Q unchanged at 92.98. The trailing noise patterns (VISA, BANK, etc.) don't catch anything stage1 misses. State uppercase_exceptions (VIC, QLD, etc.) ensure proper title casing. Kept for cosmetic improvements.
