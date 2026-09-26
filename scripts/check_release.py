#!/usr/bin/env python3
"""Validate upstream provenance and ensure version tags match Cargo.toml."""
import os
from pathlib import Path
import runpy
import tomllib

root = Path(__file__).resolve().parents[1]
runpy.run_path(str(root / 'scripts/check_upstream.py'), run_name='__main__')
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
ref = os.environ.get('GITHUB_REF', '')
if ref.startswith('refs/tags/') and ref != 'refs/tags/v' + version:
    raise SystemExit(f'Release tag must be v{version}, got {ref}')
print(f'Release version: {version}')
