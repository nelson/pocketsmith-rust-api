"""Stage 4: Identity-based normalisation."""

from typing import Optional
import re
import yaml
from pathlib import Path


def load_rules(rules_dir: str) -> dict:
    """Load stage 4 rules from YAML."""
    path = Path(rules_dir) / "stage4_rules.yaml"
    with open(path) as f:
        rules = yaml.safe_load(f)

    # Pre-compile merchant mapping patterns
    for m in rules.get("merchant_mappings", []):
        m["_re"] = re.compile(m["pattern"], re.IGNORECASE)

    # Pre-compile strip patterns
    for s in rules.get("strip_patterns", []):
        s["_re"] = re.compile(s["pattern"], re.IGNORECASE)

    # Pre-compile transfer entity patterns
    for t in rules.get("transfer_entity_extraction", []):
        t["_re"] = re.compile(t["pattern"], re.IGNORECASE)

    # Pre-compile banking entity patterns
    for b in rules.get("banking_entity_extraction", []):
        b["_re"] = re.compile(b["pattern"], re.IGNORECASE)

    # Pre-compile banking identity mappings
    for b in rules.get("banking_identity_mappings", []):
        b["_re"] = re.compile(b["pattern"], re.IGNORECASE)

    return rules


def _apply_merchant_mappings(payee: str, rules: dict) -> Optional[str]:
    """Try merchant identity mappings. Returns canonical form or None."""
    for mapping in rules.get("merchant_mappings", []):
        m = mapping["_re"].search(payee)
        if m:
            canonical = mapping["canonical"]
            # Replace {N} placeholders with capture groups
            for i in range(1, 10):
                placeholder = f"{{{i}}}"
                if placeholder in canonical:
                    try:
                        canonical = canonical.replace(placeholder, m.group(i).strip())
                    except IndexError:
                        canonical = canonical.replace(placeholder, "")
            return canonical.strip()
    return None


def _strip_suffixes(payee: str, rules: dict) -> str:
    """Strip PTY LTD and similar suffixes."""
    for s in rules.get("strip_patterns", []):
        payee = s["_re"].sub("", payee)
    return payee.strip()


def _canonicalise_person(name: str, rules: dict) -> str:
    """Map person name variants to canonical form."""
    persons = rules.get("persons", {})
    # Try exact match first
    if name in persons:
        return persons[name]
    # Try case-insensitive
    upper = name.upper()
    for variant, canonical in persons.items():
        if variant.upper() == upper:
            return canonical
    # Try stripping whitespace variations
    stripped = " ".join(name.split())
    if stripped in persons:
        return persons[stripped]
    return name


def _extract_transfer_entity(original_payee: str, rules: dict) -> Optional[str]:
    """Extract person/entity name from transfer payee."""
    for t in rules.get("transfer_entity_extraction", []):
        m = t["_re"].search(original_payee)
        if m:
            entity = m.group(t.get("group", 1)).strip().rstrip(",;.")
            prefix = t.get("prefix", "")
            if prefix:
                return f"{prefix} {entity}"
            return entity
    return None


def _extract_banking_entity(original_payee: str, rules: dict) -> Optional[str]:
    """Extract entity from banking operation payee."""
    for b in rules.get("banking_entity_extraction", []):
        m = b["_re"].search(original_payee)
        if m:
            entity = m.group(b.get("group", 1)).strip()
            prefix = b.get("prefix", "")
            if prefix:
                return f"{prefix} {entity}"
            return entity
    return None


def _resolve_employer(entity: str, rules: dict) -> str:
    """Map employer name to canonical salary form."""
    for emp in rules.get("employers", []):
        for pattern in emp["patterns"]:
            if pattern.upper() in entity.upper():
                return emp["canonical"]
    return entity


def apply(payee: str, original_payee: str, payee_type: str, metadata: dict, rules: dict) -> tuple[str, dict]:
    """Apply identity-based normalisation. Returns (normalised_payee, metadata)."""

    # Handle salary type
    if payee_type == "salary":
        entity = metadata.get("extracted_entity", payee)
        canonical = _resolve_employer(entity, rules)
        metadata["identity"] = canonical
        return canonical, metadata

    # Handle transfers: extract person name, but check for known employers first
    if payee_type in ("transfer_in", "transfer_out"):
        entity = _extract_transfer_entity(original_payee, rules)
        if not entity:
            entity = metadata.get("extracted_entity", payee)
        # For transfer_in: check if this is a known employer (e.g., "From APPLE PTY LTD")
        if payee_type == "transfer_in":
            resolved = _resolve_employer(entity, rules)
            if resolved != entity:
                metadata["identity"] = resolved
                return resolved, metadata
        # For transfer_out to a known employer: it's a payment/donation, not salary
        if payee_type == "transfer_out":
            resolved = _resolve_employer(entity, rules)
            if resolved != entity:
                # Outgoing transfer to employer = donation/payment
                out_label = re.sub(r'\(.*?\)', '(Donation)', resolved)
                metadata["identity"] = out_label
                return out_label, metadata
        # Otherwise, canonicalise as a person name
        canonical = _canonicalise_person(entity, rules)
        metadata["identity"] = canonical
        return canonical, metadata

    # Handle banking operations
    if payee_type == "banking_operation":
        # Strip known prefixes for matching (e.g., "Cafes - ", "Return DD/MM/YY, ")
        stripped_orig = original_payee
        if stripped_orig.startswith("Cafes - "):
            stripped_orig = stripped_orig[8:]
        stripped_orig = re.sub(r'^Return \d{2}/\d{2}/\d{2},?\s*', '', stripped_orig)

        # First, check banking identity mappings (most specific)
        for b in rules.get("banking_identity_mappings", []):
            if b["_re"].search(original_payee) or b["_re"].search(stripped_orig):
                canonical = b["canonical"]
                metadata["identity"] = canonical
                return canonical, metadata

        entity = _extract_banking_entity(original_payee, rules)
        if entity:
            # Check if it's an employer
            resolved = _resolve_employer(entity, rules)
            if resolved != entity:
                metadata["identity"] = resolved
                return resolved, metadata
            metadata["identity"] = entity
            return entity, metadata
        # Fallback: use extracted_entity from stage 2
        entity = metadata.get("extracted_entity", payee)
        if entity:
            metadata["identity"] = entity
            return entity, metadata
        return payee, metadata

    # Handle person type
    if payee_type == "person":
        canonical = _canonicalise_person(payee, rules)
        metadata["identity"] = canonical
        return canonical, metadata

    # Handle merchant type
    if payee_type == "merchant":
        # Try merchant identity mappings
        canonical = _apply_merchant_mappings(payee, rules)
        if canonical:
            metadata["identity"] = canonical
            return canonical, metadata

        # Strip PTY LTD etc
        result = _strip_suffixes(payee, rules)
        if result != payee:
            metadata["pty_stripped"] = True

        return result, metadata

    # Default: return as-is
    return payee, metadata
