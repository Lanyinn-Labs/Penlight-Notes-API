#!/usr/bin/env python3
"""Download a credential-free ournotes-account@1 snapshot for offline tools."""

import argparse
import json
import os
from pathlib import Path
import tempfile
import urllib.error
import urllib.request


def export_account(api_url, output, api_key):
    if not api_key:
        raise ValueError("API Key environment variable is empty")
    request = urllib.request.Request(
        api_url.rstrip("/") + "/api/jp/user/export", headers={"X-API-Key": api_key}
    )
    try:
        with urllib.request.urlopen(request, timeout=40) as response:
            document = json.load(response)
    except urllib.error.HTTPError as error:
        raise ValueError(f"Account export failed: HTTP {error.code}") from None
    except (urllib.error.URLError, json.JSONDecodeError):
        raise ValueError("Account export request or JSON decoding failed") from None
    if not isinstance(document, dict) or document.get("schema") != "ournotes-account@1":
        raise ValueError("Unexpected account export schema")
    output = Path(output)
    if output.is_symlink():
        raise ValueError("Export destination must not be a symlink")
    output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(
        prefix="." + output.name + "-", dir=output.parent
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(document, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, output)
        output.chmod(0o600)
    finally:
        Path(temporary).unlink(missing_ok=True)
    return document


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-url", default="http://127.0.0.1:8081")
    parser.add_argument("--api-key-env", default="PENLIGHT_API_KEY")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        document = export_account(
            args.api_url, args.output, os.environ.get(args.api_key_env)
        )
    except (ValueError, OSError) as error:
        parser.exit(1, str(error) + "\n")
    print(
        f"Exported {document['profile']['profile_id']}: "
        f"{len(document['members'])} members, {len(document['snapshots'])} snapshots -> {args.output}"
    )


if __name__ == "__main__":
    main()
