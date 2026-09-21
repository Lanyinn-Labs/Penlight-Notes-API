#!/usr/bin/env python3
"""Generate an APK inventory with architecture information and a SHA-256 checksum."""
import argparse
import hashlib
import json
from pathlib import Path
from zipfile import BadZipFile, ZipFile


def inspect(path: Path, region: str) -> dict:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    with ZipFile(path) as archive:
        entries = archive.infolist()
        names = {entry.filename for entry in entries}
        markers = ("libil2cpp.so", "libunity.so", "global-metadata.dat", "globalgamemanagers")
        return {
            "region": region,
            "filename": path.name,
            "size_bytes": path.stat().st_size,
            "sha256": digest.hexdigest(),
            "entry_count": len(entries),
            "architectures": sorted({name.split("/")[1] for name in names if name.startswith("lib/") and name.endswith(".so")}),
            "has_android_manifest": "AndroidManifest.xml" in names,
            "has_il2cpp": any(name.endswith("/libil2cpp.so") for name in names),
            "has_il2cpp_metadata": any(name.endswith("/global-metadata.dat") for name in names),
            "relevant_entries": [
                {"path": entry.filename, "size_bytes": entry.file_size}
                for entry in entries if entry.filename.endswith(markers)
            ],
            "notes": [
                "Region is specified by the --region argument.",
                "Inspection scope: archive entries and file checksum. Manifest and protocol parsing are not included.",
            ],
        }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--region", required=True, choices=["global", "jp"])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        result = json.dumps(inspect(args.apk, args.region), ensure_ascii=False, indent=2) + "\n"
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(result, encoding="utf-8")
        else:
            print(result, end="")
    except (OSError, BadZipFile) as error:
        parser.exit(1, f"APK inspection failed: {error}\n")


if __name__ == "__main__":
    main()
