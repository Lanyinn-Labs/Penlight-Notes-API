#!/usr/bin/env python3
"""Verify the retained upstream notices and MIT license for the secondary development crate."""

import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "crates/sirius-api-proxy"
record = json.loads((root / "UPSTREAM.json").read_text())
assert record["license"] == "MIT"
assert record["repository"] == "https://github.com/Srirus-Project/sirius-api-proxy"
license_text = (root / "LICENSE").read_text()
assert "Haruki Dev Team" in license_text and "Sirius Project" in license_text
assert "Copyright 2008 Google Inc." in (root / "LICENSE-protobuf").read_text()
print("Verified Sirius proxy secondary development attribution and original license notices")
