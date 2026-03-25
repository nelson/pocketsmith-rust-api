# Iteration 003

## Hypothesis
Always apply smart title case in stage5 (don't skip mixed-case inputs) will fix 45 near-duplicate clusters caused by inconsistent casing, improving S_dedup.

## Change
Modify stage5_cleanup.py to always apply _smart_title_case, adding case-sensitive brand exceptions.
