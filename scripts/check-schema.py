#!/usr/bin/env python3
"""Validates every fixtures/docs/*.json, and every ```json block in
packages/schema/EXAMPLES.md, against packages/schema/document.v1.json.

Together with crates/core/tests/schema_parity.rs this keeps the JSON Schema
(the contract) and the Rust structs in agreement.
Requires: pip install jsonschema
"""
import json
import pathlib
import sys

from jsonschema import Draft202012Validator

root = pathlib.Path(__file__).resolve().parent.parent
schema = json.loads((root / "packages/schema/document.v1.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)

docs = [(p.name, json.loads(p.read_text())) for p in sorted((root / "fixtures/docs").glob("*.json"))]
examples = (root / "packages/schema/EXAMPLES.md").read_text().split("```json\n")[1:]
docs += [(f"EXAMPLES.md #{i + 1}", json.loads(b.split("\n```")[0])) for i, b in enumerate(examples)]

failed = 0
for name, doc in docs:
    errors = sorted(validator.iter_errors(doc), key=lambda e: list(e.path))
    for e in errors[:5]:
        path = "".join(f"[{p}]" if isinstance(p, int) else f".{p}" for p in e.path)
        print(f"{name}: {path or '<root>'}: {e.message[:200]}")
    failed += bool(errors)

print(f"schema check: {len(docs) - failed}/{len(docs)} documents conform ({len(examples)} from EXAMPLES.md)")
sys.exit(1 if failed else 0)
