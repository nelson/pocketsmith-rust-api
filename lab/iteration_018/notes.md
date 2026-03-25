# Iteration 018: Build + run Rust fuzzy clusterer

**Hypothesis**: Trigram Jaccard clustering at threshold 0.8 will identify mergeable long-tail payees.

**Result**: 113 clusters found from 4,252 long-tail payees (140 edges). Diagnostic only — no metric change. Cluster quality is high with clear merge candidates:

Top opportunities for future iterations:
1. **Date-prefixed duplicates**: OMF International with date prefixes → add date prefix stripping to stage1
2. **Truncation variants**: Gnt Services, Wok N Bbq Chinese Rest, Burwood Discount Chemist, Papparich, Bespoke Letterpress, Duo, Cornerstone Presbyterian, Yun Zhou → add to stage3 truncation dictionary
3. **Receipt noise**: Tam S & Tam L with receipt numbers, International Transaction Fee → improve suffix stripping
4. **ATM variants**: ATM 210 Burwood A/B/C → could add ATM location normalisation

These clusters can drive rules in iterations 019+.
