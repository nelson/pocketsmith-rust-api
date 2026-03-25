# Iteration 002

## Hypothesis
Adding Transport NSW merchant_mappings to stage4 will consolidate ~1340 transactions across 15+ variants into a single "Transport NSW" payee, improving S_dedup by reducing unique normalised count.

## Change
Add merchant_mappings for all Transport NSW / TFNSW / Opal variants to map to "Transport NSW".
