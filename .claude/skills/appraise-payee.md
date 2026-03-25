---
name: appraise-payee
description: Classify payee strings and build entity knowledge base
---

# Appraise Payee

Classify payee strings and identify entities. Accumulate knowledge across iterations.

## Usage

Given one or more payee strings, determine:
1. **Type**: merchant, person, transfer_in, transfer_out, salary, banking_operation, generic
2. **Entity name**: The canonical name of the merchant, person, or organisation
3. **Confidence**: high, medium, low
4. **Category hint**: If merchant, what type of business (e.g., "Supermarket", "Restaurant", "Cafe")

## Process

1. **Run through the pipeline** to see current classification:
   ```python
   from tools.normalise.pipeline import load_all_rules, normalise_payee
   rules = load_all_rules('tools/normalise/rules')
   normalised, metadata = normalise_payee(payee_string, rules)
   ```

2. **Assess** whether the classification is correct using your knowledge of:
   - Australian merchants and locations
   - Common payment patterns (Direct Credit = incoming, Direct Debit = outgoing)
   - Person name patterns (Title Case, 2-4 words, no numbers)

3. **Update knowledge base** (`tools/normalise/knowledge.yaml`):
   - Add new merchants with category hints and aliases
   - Add new persons with name variants
   - Add new employer patterns

4. **Report** findings in structured format:
   ```
   Payee: ORIGINAL STRING
   Type: merchant
   Entity: Canonical Name
   Confidence: high
   Category: Restaurant
   Notes: Korean BBQ restaurant in Strathfield
   ```

## When to use

- During the normalise loop when unknown payees are encountered
- When the user asks about specific payees
- To batch-classify groups of similar payees (e.g., all Weixin payees)

## Knowledge base location

`tools/normalise/knowledge.yaml` — read before appraising, update after new discoveries.
