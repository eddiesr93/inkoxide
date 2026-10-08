#!/usr/bin/env python3
"""Validates every fixtures/docs/*.json against schema/document.v1.json.

Together with crates/core/tests/schema_parity.rs this keeps the JSON Schema
(the contract) and the Rust structs in agreement.
Requires: pip install jsonschema
"""
import json
import pathlib
import sys

from jsonschema import Draft202012Validator

root = pathlib.Path(__file__).resolve().parent.parent
schema = json.loads((root / "schema/document.v1.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)

failed = 0
docs = sorted((root / "fixtures/docs").glob("*.json"))
for doc in docs:
    errors = sorted(validator.iter_errors(json.loads(doc.read_text())), key=lambda e: list(e.path))
    for e in errors[:5]:
        path = "".join(f"[{p}]" if isinstance(p, int) else f".{p}" for p in e.path)
        print(f"{doc.name}: {path or '<root>'}: {e.message[:200]}")
    failed += bool(errors)

print(f"schema check: {len(docs) - failed}/{len(docs)} fixtures conform")
sys.exit(1 if failed else 0)
