---
name: measure-quality
description: Score normalised payees using the quality metric. Must stay constant across iterations.
---

# Measure Quality

Score a normalised payee set using the composite quality metric.

## How to run

```bash
python3 -m tools.normalise.measure --db pocketsmith.db --rules tools/normalise/rules --disambiguation tools/normalise/disambiguation.yaml
```

For baseline (no normalisation):
```bash
python3 -m tools.normalise.measure --db pocketsmith.db --baseline --disambiguation tools/normalise/disambiguation.yaml
```

Save to file:
```bash
python3 -m tools.normalise.measure --db pocketsmith.db --rules tools/normalise/rules --disambiguation tools/normalise/disambiguation.yaml --output lab/iteration_NNN/metrics.json
```

## Metric formula

```
Q = 0.30 * S_disambig + 0.25 * S_dedup + 0.20 * S_entity + 0.15 * S_noise + 0.10 * S_truncation
```

| Score | Weight | What it measures |
|-------|--------|-----------------|
| S_disambig | 30% | Hard test cases pass/fail (disambiguation.yaml) |
| S_dedup | 25% | Deduplication rate (sigmoid-scaled, over-merge penalty) |
| S_entity | 20% | Entity type identification rate (non-generic) |
| S_noise | 15% | Noise removal rate (card numbers, dates, receipts) |
| S_truncation | 10% | Truncation expansion rate |

## Stopping criteria

Stop when **both**:
1. S_disambig = 100
2. Q >= 80

Safety valve: max 20 iterations.

## IMPORTANT

This metric definition MUST NOT CHANGE between iterations. The formula, weights, and scoring functions in `tools/normalise/measure.py` are fixed. Only disambiguation test cases in `disambiguation.yaml` may be added (never removed or weakened).
