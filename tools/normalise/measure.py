"""Quality metric scorer for payee normalisation."""

import argparse
import json
import math
import re
import sqlite3
from collections import Counter

import yaml
from pathlib import Path

from .db_utils import open_db, get_all_payees
from .pipeline import load_all_rules, normalise_payee


def load_disambiguation(path: str) -> dict:
    """Load disambiguation test cases."""
    with open(path) as f:
        return yaml.safe_load(f)


def score_disambiguation(results: list[dict], disambig: dict, conn: sqlite3.Connection) -> tuple[float, list[dict]]:
    """Score S_disambig: do hard test cases pass?

    Returns (score 0-100, list of failing test cases).
    """
    test_cases = disambig.get("test_cases", [])
    if not test_cases:
        return 100.0, []

    # Build a lookup from original_payee -> normalised payee
    orig_to_norm = {}
    for r in results:
        orig_to_norm[r["original"]] = r["final"]

    # Also query DB for pattern-based lookups
    all_originals = list(orig_to_norm.keys())

    passing = 0
    failures = []

    for tc in test_cases:
        must_match = tc.get("must_match", False)

        # Find normalised payees matching group_a patterns
        group_a_norms = set()
        for pat in tc["group_a"]["patterns"]:
            regex = re.compile(pat, re.IGNORECASE)
            for orig, norm in orig_to_norm.items():
                if regex.search(orig):
                    group_a_norms.add(norm)

        # Find normalised payees matching group_b patterns
        group_b_norms = set()
        for pat in tc["group_b"]["patterns"]:
            regex = re.compile(pat, re.IGNORECASE)
            for orig, norm in orig_to_norm.items():
                if regex.search(orig):
                    group_b_norms.add(norm)

        if not group_a_norms or not group_b_norms:
            # Can't test if no matching payees found
            passing += 1
            continue

        if must_match:
            # Groups should normalise to the same value(s)
            all_norms = group_a_norms | group_b_norms
            if len(all_norms) == 1:
                passing += 1
            else:
                failures.append({
                    "name": tc["name"],
                    "expected": "match",
                    "group_a": list(group_a_norms),
                    "group_b": list(group_b_norms),
                })
        else:
            # Groups must normalise to different values
            overlap = group_a_norms & group_b_norms
            if not overlap:
                passing += 1
            else:
                failures.append({
                    "name": tc["name"],
                    "expected": "differ",
                    "overlap": list(overlap),
                    "group_a": list(group_a_norms),
                    "group_b": list(group_b_norms),
                })

    score = 100.0 * passing / len(test_cases) if test_cases else 100.0
    return score, failures


def score_dedup(results: list[dict]) -> float:
    """Score S_dedup: deduplication rate with sigmoid scaling.

    For grouped merchants (location variants), count the group name
    instead of individual canonicals toward unique-normalised count.
    """
    originals = {r["original"] for r in results}

    # Build grouped normalised set: replace grouped merchant canonicals with group name
    normalised = set()
    for r in results:
        group = r.get("merchant_group")
        if group:
            normalised.add(group)
        else:
            normalised.add(r["final"])

    raw_unique = len(originals)
    norm_unique = len(normalised)

    if raw_unique == 0:
        return 0.0

    reduction = (raw_unique - norm_unique) / raw_unique

    # Sigmoid scaling: optimal around 0.70, with diminishing returns
    # S = 100 / (1 + exp(-12 * (reduction - 0.35)))
    score = 100.0 / (1.0 + math.exp(-12.0 * (reduction - 0.35)))

    # Over-merge penalty: if any single normalised payee maps to >200 originals, subtract 20
    norm_counts = Counter(r["final"] for r in results)
    max_count = max(norm_counts.values()) if norm_counts else 0
    if max_count > 200:
        score = max(0, score - 20)

    return min(100.0, score)


def score_entity(results: list[dict]) -> float:
    """Score S_entity: entity identification rate."""
    unique_payees = {r["final"] for r in results}
    # Build final -> type mapping (take first seen)
    final_types = {}
    for r in results:
        if r["final"] not in final_types:
            final_types[r["final"]] = r.get("type", "merchant")

    if not unique_payees:
        return 0.0

    identified = sum(1 for t in final_types.values() if t != "generic")
    return 100.0 * identified / len(unique_payees)


def score_noise(results: list[dict]) -> float:
    """Score S_noise: noise removal rate."""
    noise_patterns = [
        re.compile(r',?\s*Card xx\d{4}'),
        re.compile(r'Visa Purchase\s*-\s*Receipt'),
        re.compile(r'Visa Refund\s*-\s*Receipt'),
        re.compile(r'Osko Payment.*Receipt'),
        re.compile(r'Tap and Pay xx\d{4}'),
        re.compile(r'Value [Dd]ate:?\s*\d{2}/\d{2}/\d{4}'),
        re.compile(r'Card \d{6}x{6}\d{4}'),
        re.compile(r'Receipt \d+'),
        re.compile(r'- Deposit - Receipt'),
    ]

    with_noise = 0
    noise_removed = 0

    for r in results:
        orig = r["original"]
        final = r["final"]
        has_noise = any(p.search(orig) for p in noise_patterns)
        if has_noise:
            with_noise += 1
            still_noisy = any(p.search(final) for p in noise_patterns)
            if not still_noisy:
                noise_removed += 1

    if with_noise == 0:
        return 100.0
    return 100.0 * noise_removed / with_noise


def score_truncation(results: list[dict]) -> float:
    """Score S_truncation: truncation fix rate."""
    expanded = sum(1 for r in results if r.get("truncations_expanded"))
    # Count payees that still have known truncation patterns
    truncation_indicators = [
        re.compile(r'\bSTRATHFI\b', re.IGNORECASE),
        re.compile(r'\bBURWOO\b', re.IGNORECASE),
        re.compile(r'\bNORTH RY\b', re.IGNORECASE),
        re.compile(r'\bPHARMCY\b', re.IGNORECASE),
        re.compile(r'\bMKTPL\b', re.IGNORECASE),
        re.compile(r'\bSUPERMARKE\b', re.IGNORECASE),
    ]

    still_truncated = 0
    for r in results:
        if any(p.search(r["final"]) for p in truncation_indicators):
            still_truncated += 1

    total_truncation_cases = expanded + still_truncated
    if total_truncation_cases == 0:
        return 100.0
    return 100.0 * expanded / total_truncation_cases


def compute_metrics(results: list[dict], disambig_path: str, db_path: str) -> dict:
    """Compute all quality metrics. Returns dict with scores."""
    disambig = load_disambiguation(disambig_path)
    conn = open_db(db_path)

    s_disambig, disambig_failures = score_disambiguation(results, disambig, conn)
    s_dedup = score_dedup(results)
    s_entity = score_entity(results)
    s_noise = score_noise(results)
    s_truncation = score_truncation(results)

    # Composite score
    q = (0.30 * s_disambig + 0.25 * s_dedup + 0.20 * s_entity +
         0.15 * s_noise + 0.10 * s_truncation)

    conn.close()

    # Stats
    originals = {r["original"] for r in results}
    normalised = {r["final"] for r in results}
    type_counts = Counter(r.get("type", "unknown") for r in results)

    # Long-tail analysis
    norm_counter = Counter(r["final"] for r in results)
    orig_counter = Counter(r["original"] for r in results)
    long_tail_before = sum(1 for v in orig_counter.values() if v <= 10)
    long_tail_after = sum(1 for v in norm_counter.values() if v <= 10)

    return {
        "composite_score": round(q, 2),
        "sub_scores": {
            "S_disambig": round(s_disambig, 2),
            "S_dedup": round(s_dedup, 2),
            "S_entity": round(s_entity, 2),
            "S_noise": round(s_noise, 2),
            "S_truncation": round(s_truncation, 2),
        },
        "stats": {
            "total_transactions": len(results),
            "unique_original": len(originals),
            "unique_normalised": len(normalised),
            "reduction_pct": round(100 * (len(originals) - len(normalised)) / len(originals), 1) if originals else 0,
            "type_distribution": dict(type_counts),
        },
        "long_tail": {
            "unique_before": len(orig_counter),
            "unique_after": len(norm_counter),
            "long_tail_before": long_tail_before,
            "long_tail_after": long_tail_after,
            "txn_count_after": sum(v for v in norm_counter.values() if v <= 10),
            "singleton_count": sum(1 for v in norm_counter.values() if v == 1),
            "clusters_found": 0,
            "clusters_merged": 0,
        },
        "disambig_failures": disambig_failures,
    }


def main():
    parser = argparse.ArgumentParser(description="Quality metric scorer")
    parser.add_argument("--db", required=True, help="Path to SQLite database")
    parser.add_argument("--rules", default="tools/normalise/rules", help="Path to rules directory")
    parser.add_argument("--disambiguation", default="tools/normalise/disambiguation.yaml")
    parser.add_argument("--output", help="Output JSON to file")
    parser.add_argument("--baseline", action="store_true", help="Measure baseline (no normalisation)")
    args = parser.parse_args()

    conn = open_db(args.db)
    payees = get_all_payees(conn)
    conn.close()

    if args.baseline:
        # Baseline: original == final, type = unknown
        results = [{"original": op, "final": op, "type": "merchant"} for _, _, op in payees]
    else:
        rules = load_all_rules(args.rules)
        results = []
        for txn_id, current_payee, original_payee in payees:
            source = original_payee if original_payee else current_payee
            normalised, metadata = normalise_payee(source, rules)
            results.append(metadata)

    metrics = compute_metrics(results, args.disambiguation, args.db)

    output = json.dumps(metrics, indent=2)
    if args.output:
        with open(args.output, "w") as f:
            f.write(output)
        print(f"Metrics written to {args.output}")
    else:
        print(output)


if __name__ == "__main__":
    main()
