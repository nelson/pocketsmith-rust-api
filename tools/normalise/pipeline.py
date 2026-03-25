"""5-stage payee normalisation pipeline orchestrator."""

import argparse
import json
import sqlite3
import sys
import random
from pathlib import Path

from . import stage1_strip, stage2_classify, stage3_expand, stage4_identity, stage5_cleanup
from .db_utils import clone_db, change_reason, get_all_payees, batch_update_payees, open_db, HISTORY_SCHEMA


def load_all_rules(rules_dir: str) -> dict:
    """Load rules for all stages."""
    return {
        "stage1": stage1_strip.load_rules(rules_dir),
        "stage2": stage2_classify.load_rules(rules_dir),
        "stage3": stage3_expand.load_rules(rules_dir),
        "stage4": stage4_identity.load_rules(rules_dir),
        "stage5": stage5_cleanup.load_rules(rules_dir),
    }


def normalise_payee(original_payee: str, rules: dict) -> tuple[str, dict]:
    """Run a single payee through all 5 stages. Returns (normalised, metadata)."""
    # Stage 1: Strip prefixes and suffixes
    s1_out, s1_meta = stage1_strip.apply(original_payee, rules["stage1"])
    metadata = {"original": original_payee, **s1_meta}
    metadata["after_stage1"] = s1_out

    # Stage 2: Classify type (uses original for pattern matching)
    payee_type, metadata = stage2_classify.apply(original_payee, s1_out, metadata, rules["stage2"])
    metadata["after_stage2"] = s1_out  # Stage 2 doesn't modify the payee string

    # Stage 3: Expand truncations
    s3_out, metadata = stage3_expand.apply(s1_out, metadata, rules["stage3"])
    metadata["after_stage3"] = s3_out

    # Stage 4: Identity-based normalisation
    s4_out, metadata = stage4_identity.apply(s3_out, original_payee, payee_type, metadata, rules["stage4"])
    metadata["after_stage4"] = s4_out

    # Stage 5: Final cleanup
    s5_out, metadata = stage5_cleanup.apply(s4_out, metadata, rules["stage5"])
    metadata["final"] = s5_out

    return s5_out, metadata


def run_pipeline(db_path: str, rules_dir: str, reason: str = "normalise",
                 dry_run: bool = False) -> list[dict]:
    """Run the full pipeline on a database. Returns list of result dicts."""
    rules = load_all_rules(rules_dir)
    conn = open_db(db_path)
    # Ensure history tables exist
    conn.executescript(HISTORY_SCHEMA)

    payees = get_all_payees(conn)
    results = []
    updates = []

    for txn_id, current_payee, original_payee in payees:
        source = original_payee if original_payee else current_payee
        normalised, metadata = normalise_payee(source, rules)
        metadata["txn_id"] = txn_id
        metadata["current_payee"] = current_payee
        results.append(metadata)

        if normalised != current_payee:
            updates.append((normalised, txn_id))

    if not dry_run and updates:
        with change_reason(conn, reason):
            changed = batch_update_payees(conn, updates)
            conn.commit()
        print(f"Updated {changed} transactions (of {len(updates)} changes detected)")
    elif dry_run:
        print(f"Dry run: would update {len(updates)} of {len(payees)} transactions", file=sys.stderr)

    conn.close()
    return results


def diff_sample(results: list[dict], n: int = 50) -> list[dict]:
    """Get a random sample of changed payees for review."""
    changed = [r for r in results if r["original"] != r["final"]]
    sample = random.sample(changed, min(n, len(changed)))
    return [
        {
            "original": r["original"],
            "final": r["final"],
            "type": r.get("type", "unknown"),
            "stages": {
                "s1": r.get("after_stage1", ""),
                "s3": r.get("after_stage3", ""),
                "s4": r.get("after_stage4", ""),
            },
        }
        for r in sample
    ]


def main():
    parser = argparse.ArgumentParser(description="Payee normalisation pipeline")
    parser.add_argument("--db", required=True, help="Path to SQLite database")
    parser.add_argument("--rules", default="tools/normalise/rules", help="Path to rules directory")
    parser.add_argument("--reason", default="normalise", help="Change reason for history tracking")
    parser.add_argument("--dry-run", action="store_true", help="Don't write changes")
    parser.add_argument("--diff-sample", type=int, metavar="N", help="Output N random changed payees as JSON")
    parser.add_argument("--output", help="Output results JSON to file")
    args = parser.parse_args()

    results = run_pipeline(args.db, args.rules, args.reason, args.dry_run or bool(args.diff_sample))

    if args.diff_sample:
        sample = diff_sample(results, args.diff_sample)
        print(json.dumps(sample, indent=2))
    elif args.output:
        with open(args.output, "w") as f:
            json.dump(results, f, indent=2)
        print(f"Results written to {args.output}")


if __name__ == "__main__":
    main()
