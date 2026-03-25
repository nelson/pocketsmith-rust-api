# Iteration 017: Title prefix stripping (MR/MRS/MS/DR)

**Hypothesis**: Stripping MR/MRS/MS/DR title prefixes will collapse person-name variants.

**Result**: Q unchanged at 92.98. Zero metric impact. Found false positive: "MR YUM*" is a merchant (restaurant ordering platform), not a title prefix. Discarded to avoid risk of merchant name corruption.
