"""Stage 3: Expand and complete truncated words."""

import re
import yaml
from pathlib import Path


def load_rules(rules_dir: str) -> dict:
    """Load stage 3 rules from YAML."""
    path = Path(rules_dir) / "stage3_rules.yaml"
    with open(path) as f:
        rules = yaml.safe_load(f)
    # Merge all dictionaries and sort by key length descending (longest match first)
    merged = {}
    for section in ["suburbs", "words", "merchants"]:
        for k, v in rules.get(section, {}).items():
            merged[k.upper()] = v
    # Sort by length descending for longest-prefix matching
    rules["_merged"] = dict(sorted(merged.items(), key=lambda x: len(x[0]), reverse=True))
    # Pre-compile patterns: match truncated word at word boundary
    rules["_patterns"] = []
    for truncated, full in rules["_merged"].items():
        # Only expand if the truncated form != full form (avoid pointless replacements)
        if truncated != full.upper():
            pattern = re.compile(r'\b' + re.escape(truncated) + r'\b', re.IGNORECASE)
            rules["_patterns"].append((pattern, full.upper(), truncated))
    return rules


def apply(payee: str, metadata: dict, rules: dict) -> tuple[str, dict]:
    """Expand truncated words in payee. Returns (expanded_payee, metadata)."""
    result = payee
    expansions = []

    for pattern, replacement, truncated in rules["_patterns"]:
        if pattern.search(result):
            result = pattern.sub(replacement, result)
            expansions.append(f"{truncated}->{replacement}")

    if expansions:
        metadata["truncations_expanded"] = expansions

    return result, metadata
