#!/usr/bin/env python3
"""Run the project's offline-safe source, lint and regression checks."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import os

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="Use cached Cargo dependencies only")
    args = parser.parse_args()
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin" / ("cargo.exe" if os.name == "nt" else "cargo"))
    options = ["--locked"] + (["--offline"] if args.offline else [])
    commands = [
        [sys.executable, "scripts/check_upstream.py"],
        [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"],
        [cargo, "fmt", "--check"],
        [cargo, "check", *options, "--all-targets"],
        [cargo, "clippy", *options, "--all-targets", "--", "-D", "warnings"],
        [cargo, "test", *options],
        ["git", "diff", "--check"],
    ]
    for command in commands:
        print("Checking: " + " ".join(command), flush=True)
        subprocess.run(command, cwd=ROOT, check=True)
    print("All project checks passed")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
