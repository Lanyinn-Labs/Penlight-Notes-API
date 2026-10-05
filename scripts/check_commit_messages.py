#!/usr/bin/env python3
"""Check Conventional Commit titles and the separation of optional commit bodies."""
import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SUBJECT = re.compile(r"(?:feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(?:\([a-z0-9][a-z0-9._/-]*\))?!?: [^\s].*")


def validate(message):
    lines = message.splitlines()
    if not lines or not SUBJECT.fullmatch(lines[0]):
        return "expected type(scope): summary (scope is optional)"
    if len(lines[0]) > 72:
        return "title exceeds 72 characters"
    if lines[0].endswith((".", "。")) or lines[0] != lines[0].rstrip():
        return "title must not end in punctuation or whitespace"
    if len(lines) > 1 and lines[1]:
        return "separate the title and body with a blank line"
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--file", type=Path, help="Prepared commit message file")
    source.add_argument("--range", dest="revisions", help="Git revision range, e.g. origin/main..HEAD")
    args = parser.parse_args()
    if args.file:
        messages = [(str(args.file), args.file.read_text(encoding="utf-8-sig"))]
    else:
        data = subprocess.check_output(["git", "log", "--format=%H%x00%B%x00", args.revisions, "--"], cwd=ROOT).decode("utf-8")
        parts = data.split("\0")
        messages = [(parts[i].strip(), parts[i+1]) for i in range(0, len(parts)-1, 2)]
        if not messages:
            parser.error("revision range contains no commits")
    failed = False
    for label, message in messages:
        error = validate(message)
        if error:
            print(f"{label}: {error}", file=sys.stderr)
            failed = True
    if failed:
        return 1
    print(f"Verified {len(messages)} commit message(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
