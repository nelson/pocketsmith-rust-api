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

    # Remove truncated-prefix duplicates: if word[i] is a proper prefix of
    # word[i+1] (case-insensitive, len >= 4), drop word[i].
    # Exclusions: directional/title words that legitimately prefix longer words,
    # and masked reference codes (Xxxxx patterns).
    _prefix_exclude = {"SAINT", "STREET", "MOUNT", "NORTH", "SOUTH", "EAST",
                        "WEST", "EVERY", "KING", "OVER", "UNDER", "CAMP",
                        "PORT", "GRAND", "PARK", "PALM"}
    words = result.split()
    cleaned = []
    i = 0
    while i < len(words):
        if i < len(words) - 1:
            a, b = words[i], words[i + 1]
            au, bu = a.upper(), b.upper()
            # Must be >= 4 chars, proper prefix, not in exclude set, not masked refs
            if (len(a) >= 4 and len(b) > len(a)
                    and bu.startswith(au) and au != bu
                    and au not in _prefix_exclude
                    and not au.startswith("XXXX")):
                # Skip the truncated word, keep the full one
                i += 1
                continue
        cleaned.append(words[i])
        i += 1
    result = " ".join(cleaned)

    # Smart title case — always apply for consistency across variants
    # Preserve known mixed-case brand names first
    brand_preserves = {
        "ebay": "eBay", "iphone": "iPhone", "ipad": "iPad",
        "youtube": "YouTube", "paypal": "PayPal", "doordash": "DoorDash",
        "pocketsmith": "PocketSmith", "commbank": "CommBank",
        "netbank": "NetBank",
    }
    result = _smart_title_case(result, rules["_upper_set"], rules["_lower_set"])
    # Restore brand casing
    for lower_brand, correct in brand_preserves.items():
        result = re.sub(re.escape(correct.title()), correct, result, flags=re.IGNORECASE)

    # Final trim
    result = result.strip()

    return result, metadata
