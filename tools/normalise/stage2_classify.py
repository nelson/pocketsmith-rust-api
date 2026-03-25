"""Stage 2: Type classification."""

import re
import yaml
from pathlib import Path


def load_rules(rules_dir: str) -> dict:
    """Load stage 2 rules from YAML."""
    path = Path(rules_dir) / "stage2_rules.yaml"
    with open(path) as f:
        rules = yaml.safe_load(f)
    for r in rules.get("classification_rules", []):
        r["_re"] = re.compile(r["pattern"])
        if r.get("extract_pattern"):
            r["_extract_re"] = re.compile(r["extract_pattern"])
    return rules


def apply(original_payee: str, stripped_payee: str, metadata: dict, rules: dict) -> tuple[str, dict]:
    """Classify payee type. Returns (type, updated_metadata).

    Classification is based on the ORIGINAL payee for reliable pattern matching.
    The stripped payee is returned unchanged.
    """
    for rule in rules.get("classification_rules", []):
        if rule["_re"].search(original_payee):
            payee_type = rule["type"]
            metadata["type"] = payee_type

            # Extract entity name if pattern provided
            if rule.get("_extract_re"):
                m = rule["_extract_re"].search(original_payee)
                if m:
                    metadata["extracted_entity"] = m.group(1).strip()

            return payee_type, metadata

    # Default: merchant
    metadata["type"] = "merchant"
    return "merchant", metadata
