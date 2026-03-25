"""Stage 5: Final cleanup."""

import re
import yaml
from pathlib import Path


def load_rules(rules_dir: str) -> dict:
    """Load stage 5 rules from YAML."""
    path = Path(rules_dir) / "stage5_rules.yaml"
    with open(path) as f:
        rules = yaml.safe_load(f)
    rules["_upper_set"] = {w.upper() for w in rules.get("uppercase_exceptions", [])}
    rules["_lower_set"] = {w.lower() for w in rules.get("lowercase_exceptions", [])}
    rules["_trailing"] = [
        re.compile(t["pattern"], re.IGNORECASE)
        for t in rules.get("trailing_noise", [])
    ]
    return rules


def _smart_title_case(text: str, upper_set: set, lower_set: set) -> str:
    """Title case with exceptions for acronyms and prepositions."""
    words = text.split()
    result = []
    for i, word in enumerate(words):
        upper = word.upper()
        if upper in upper_set:
            result.append(upper)
        elif word.lower() in lower_set and i > 0:
            result.append(word.lower())
        elif word.isupper() and len(word) > 1:
            # All-caps word: title case it (unless it's an acronym)
            result.append(word.title())
        elif word[0:1].isupper():
            # Already has some capitalisation, keep it
            result.append(word)
        else:
            result.append(word.title())
    return " ".join(result)


def apply(payee: str, metadata: dict, rules: dict) -> tuple[str, dict]:
    """Final cleanup. Returns (cleaned_payee, metadata)."""
    result = payee

    # Remove trailing punctuation
    result = re.sub(r'[,;.\\/\s]+$', '', result)
    # Remove leading punctuation
    result = re.sub(r'^[,;.\\/\s]+', '', result)

    # Normalise whitespace
    result = re.sub(r'\s+', ' ', result).strip()

    # Remove duplicate consecutive words (case-insensitive)
    words = result.split()
    deduped = [words[0]] if words else []
    for w in words[1:]:
        if w.upper() != deduped[-1].upper():
            deduped.append(w)
    result = " ".join(deduped)

    # Remove trailing noise patterns
    for pattern in rules["_trailing"]:
        result = pattern.sub("", result).strip()

    # Smart title case
    if result == result.upper() and len(result) > 3:
        # All caps: apply title case
        result = _smart_title_case(result, rules["_upper_set"], rules["_lower_set"])
    elif any(c.islower() for c in result) and any(c.isupper() for c in result):
        # Mixed case: leave as-is (already has intentional casing)
        pass
    else:
        result = _smart_title_case(result, rules["_upper_set"], rules["_lower_set"])

    # Final trim
    result = result.strip()

    return result, metadata
