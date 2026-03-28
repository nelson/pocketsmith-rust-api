Run one categorisation refinement iteration. Designed for `/loop 5m /categorise`.

## Steps

1. Run `cargo run --bin categorise -- --yes` to compute and apply all categorisations. Show the output.

2. Print the coverage + source distribution summary from the output.

3. Sample 5 random payees **biased toward low-frequency (1-9 txns)** that were categorised by LLM or marked Unknown. Use this SQL to find them:

```sql
SELECT t.payee, COUNT(*) as cnt, c.title as category
FROM transactions t
LEFT JOIN categories c ON t.category_id = c.id
WHERE t.payee IS NOT NULL
GROUP BY t.payee
HAVING cnt BETWEEN 1 AND 9
ORDER BY RANDOM()
LIMIT 5
```

Run this against `pocketsmith.db` using `sqlite3`.

4. For each sample, present it for human review:

```
Review 1/5: "payee name" (N txns) -> category [source: reason]
  Accept? Correct category? Better normalisation?
```

5. For any corrections the user gives:
   - Wrong category → add payee override to `rules/categorise.yaml`
   - Bad normalisation → add rule to the appropriate `rules/*.yaml` file
   - Accept → move on

6. After all reviews, print:

```
SCORE: coverage <pct>% | <n> low-freq unknown payees remaining
```

Compute coverage as the percentage of unique payees that have a non-null category_id. Compute low-freq unknown from the SQL above but filtered to where category is NULL or starts with "Uncategorised".
