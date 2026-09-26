#!/usr/bin/env python3
"""Verify the recorded, unchanged upstream source and retained notices offline."""

import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "vendor/sirius-api-proxy"
record = json.loads((root / "UPSTREAM.json").read_text())
assert record["license"] == "MIT"
assert record["repository"] == "https://github.com/Srirus-Project/sirius-api-proxy"
expected = record["files_sha256"]
actual = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
assert actual == set(expected) | {"UPSTREAM.json"}, "unrecorded or missing upstream files"
for name, digest in expected.items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, f"changed upstream file: {name}"
license_text = (root / "LICENSE").read_text()
assert "Haruki Dev Team" in license_text and "Sirius Project" in license_text
assert "Copyright 2008 Google Inc." in (root / "LICENSE-protobuf").read_text()
print(f"Verified {len(expected)} unchanged upstream files and original license notices")
