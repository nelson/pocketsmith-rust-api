# Iteration 022: Merchant consolidation from cluster output

**Hypothesis**: Adding stage4 merchant identity mappings derived from all 113 clusters will consolidate terminal codes, store numbers, truncation variants, duplicate locations, and near-duplicate merchants.

**Coverage**: ~80 new merchant_mapping rules covering clusters [2]-[112] — terminal codes (Bathers Pavilion, Sydney Airport, Apple Espresso Bar), store numbers (Kmart, Knight's Coffee, Salt Meats Cheese), truncation variants (Wok N BBQ, Three Mothers Kitchen, Double Barral Coffee), duplicate locations (Starbucks the Rocks, Single O Surry Hills, Dutch Smuggler), date variants (Estimated Investment Returns), and more.

**Result**: Q +0.13 (93.04→93.17). S_dedup +0.55 (72.39→72.94). 68 unique payees eliminated, 71 fewer long-tail, 61 fewer singletons. Largest single improvement since iter 005.
