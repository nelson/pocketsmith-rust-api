"""Stage 1: Prefix and suffix removal."""

import re
import yaml
from pathlib import Path


def load_rules(rules_dir: str) -> dict:
    """Load stage 1 rules from YAML."""
    path = Path(rules_dir) / "stage1_rules.yaml"
    with open(path) as f:
        rules = yaml.safe_load(f)
    # Pre-compile regexes
    for p in rules.get("prefixes", []):
        p["_re"] = re.compile(p["pattern"], re.IGNORECASE)
    for s in rules.get("suffixes", []):
        s["_re"] = re.compile(s["pattern"], re.IGNORECASE)
    return rules


def apply(payee: str, rules: dict) -> tuple[str, dict]:
    """Strip prefixes and suffixes. Returns (cleaned_payee, metadata)."""
    metadata = {}
    result = payee.upper()

    # Apply prefixes (first match wins)
    for rule in rules.get("prefixes", []):
        m = rule["_re"].search(result)
        if m:
            metadata["prefix_stripped"] = rule["name"]
            if rule.get("set_flag"):
                metadata[rule["set_flag"]] = True
            result = result[m.end():]
            break

    # Apply suffixes (all matching, from first to last — order matters)
    for rule in rules.get("suffixes", []):
        m = rule["_re"].search(result)
        if m:
            metadata.setdefault("suffixes_stripped", []).append(rule["name"])
            result = result[:m.start()]

    result = result.strip()
    return result, metadata
