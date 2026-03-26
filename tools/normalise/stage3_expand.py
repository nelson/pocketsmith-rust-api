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

    # Pre-compile known_locations patterns (longest first for most specific match)
    locations = rules.get("known_locations", [])
    locations_sorted = sorted(locations, key=len, reverse=True)
    rules["_location_patterns"] = []
    for loc in locations_sorted:
        pattern = re.compile(r'\b' + re.escape(loc.upper()) + r'\b', re.IGNORECASE)
        rules["_location_patterns"].append((pattern, loc))

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

    # Detect known locations in the (possibly expanded) payee
    for pattern, loc_name in rules["_location_patterns"]:
        if pattern.search(result):
            metadata["detected_location"] = loc_name
            break

    return result, metadata
