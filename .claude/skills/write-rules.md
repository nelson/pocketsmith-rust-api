---
name: write-rules
description: Generate initial normalisation rules by analysing the payee corpus
---

# Write Rules

Generate a complete set of normalisation rules for the 5-stage payee pipeline.

## What to do

1. **Read the payee corpus** from `pocketsmith.db`:
   ```bash
   sqlite3 pocketsmith.db "SELECT DISTINCT original_payee FROM transactions ORDER BY original_payee"
   ```

2. **For each stage, generate rules** by identifying patterns in the data:

   - **Stage 1** (`tools/normalise/rules/stage1_rules.yaml`): Find recurring prefixes (payment gateways like `SMP*`, `SQ *`, `ZLR*`) and suffixes (card numbers, value dates, Visa Purchase receipts, Osko receipts). Use frequency analysis to prioritise.

   - **Stage 2** (`tools/normalise/rules/stage2_rules.yaml`): Write classification patterns. Use the original payee string (before stripping). Key types: salary, transfer_in, transfer_out, banking_operation, merchant.

   - **Stage 3** (`tools/normalise/rules/stage3_rules.yaml`): Build a truncation expansion dictionary. Find truncated Australian suburb names (compare `STRATHFI` vs `STRATHFIELD` in the data) and common word truncations.

   - **Stage 4** (`tools/normalise/rules/stage4_rules.yaml`): Write identity mappings. Include person name canonicalisation, employer mappings, merchant identity mappings (e.g., `AMAZON MKTPL*` → `Amazon Marketplace`), and transfer entity extraction patterns.

   - **Stage 5** (`tools/normalise/rules/stage5_rules.yaml`): Define title case exceptions (acronyms to keep uppercase, words to keep lowercase) and trailing noise patterns.

3. **Create `tools/normalise/disambiguation.yaml`** with hard test cases that must never merge (Apple salary vs Apple Store, AFES salary vs AFES donation, Amazon Marketplace vs Prime Video, etc.)

4. **Seed `tools/normalise/knowledge.yaml`** with known entities from the user's requirements.

5. **Verify** rules parse correctly:
   ```bash
   python3 -m tools.normalise.pipeline --db pocketsmith.db --rules tools/normalise/rules --dry-run
   ```

## Key constraints

- Nelson works at Apple: `From APPLE PTY LTD` = salary, `APPLE STORE` = spending
- Sophia works at AFES: `From AFES - [numbers]` = salary, `AFES KINGSFORD` = spending, `Direct Debit AFES` = donation
- Keep store locations distinct (Woolworths Strathfield ≠ Woolworths Burwood)
- Amazon Marketplace ≠ Amazon Prime Video
- Transfers normalise to person name only (no direction prefix)
- Weixin: strip prefix, keep transliterated name

## Output

All YAML rule files in `tools/normalise/rules/`, plus `disambiguation.yaml` and `knowledge.yaml`.
