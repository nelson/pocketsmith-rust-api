---
description: Run normalise pipeline and review unmatched payees
model: haiku
user_invocable: true
allowed-tools: Bash, AskUserQuestion, Read
---

# Normalise Pipeline

Run the Rust normalisation pipeline on all original_payee values in the database, review coverage metrics, and identify payees that need new patterns.

## Workflow

### Step 1: Dry-run the normalise binary

```bash
cargo run --bin normalise -- --dry-run 2>&1
```

Parse the output sections:
- **Normalisation Summary**: classification breakdown (Merchant/Person/Employer/Other/Unclassified)
- **Merchant Coverage**: entity_name, location, and full query rates
- **Top Unclassified**: highest-transaction-count unclassified payees
- **Top Merchants Missing entity_name**: merchants without entity extraction
- **Top Merchants Missing location**: merchants without location extraction

### Step 2: Present metrics and gaps to user

Present a concise summary of:
- Classification rate (% of transactions classified)
- Entity extraction rate (% of merchants with entity_name)
- Location extraction rate (% of merchants with location)

Then use AskUserQuestion to present the top unclassified payees (up to 4 at a time) and ask which ones the user wants to add patterns for. Format each question with the raw payee string, the normalised output, and the transaction count.

Options for each: "Add merchant pattern", "Add person pattern", "Skip", "Stop reviewing"

If the user says "Stop reviewing", end the review loop.

### Step 3: Report next steps

After the user has reviewed the gaps, summarise:
- Which payees they want patterns added for
- Current coverage metrics
- Suggested next action (e.g., "add patterns to merchants.rs, then re-run /normalise")

Do NOT modify any source files or apply DB changes. This skill is for review and planning only.
