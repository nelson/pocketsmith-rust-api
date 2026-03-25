"""Knowledge base for accumulated entity information."""

import yaml
from pathlib import Path


def load(path: str) -> dict:
    """Load knowledge base from YAML."""
    p = Path(path)
    if not p.exists():
        return {"version": 1, "merchants": {}, "persons": {}, "employers": {}}
    with open(p) as f:
        return yaml.safe_load(f) or {"version": 1, "merchants": {}, "persons": {}, "employers": {}}


def save(kb: dict, path: str) -> None:
    """Save knowledge base to YAML."""
    with open(path, "w") as f:
        yaml.dump(kb, f, default_flow_style=False, allow_unicode=True, sort_keys=False)


def add_merchant(kb: dict, canonical_name: str, **kwargs) -> None:
    """Add or update a merchant in the knowledge base."""
    merchants = kb.setdefault("merchants", {})
    existing = merchants.get(canonical_name, {})
    for key, value in kwargs.items():
        if isinstance(value, list) and isinstance(existing.get(key), list):
            existing[key] = list(set(existing[key] + value))
        else:
            existing[key] = value
    merchants[canonical_name] = existing


def add_person(kb: dict, canonical_name: str, **kwargs) -> None:
    """Add or update a person in the knowledge base."""
    persons = kb.setdefault("persons", {})
    existing = persons.get(canonical_name, {})
    for key, value in kwargs.items():
        if isinstance(value, list) and isinstance(existing.get(key), list):
            existing[key] = list(set(existing[key] + value))
        else:
            existing[key] = value
    persons[canonical_name] = existing
