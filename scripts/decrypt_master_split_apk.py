#!/usr/bin/env python3
"""Validate and decrypt Master tables from the Japanese Android split APKs.

Requires py3rijndael. The FieldRVA offsets and fingerprints below were verified
against the Japanese Android build extracted on 2026-09-24. A changed build must
be checked before these offsets can be reused.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import zipfile
from pathlib import Path

from decrypt_master import decrypt_file, file_sha256


METADATA_PATH = "assets/bin/Data/Managed/Metadata/global-metadata.dat"
MASTER_ROOT = "assets/Master/"
FIELD_DATA_INDEX = {"header": 474976, "key": 474872, "iv": 474760}
FIELD_SHA256 = {
    "header": "6a8a8e599a57e654983fcc8ed280ce9bdc638f0531b99c06c5be10bd6c4c689b",
    "key": "661b3a4e30cbbe6e159c42b2f3c93193f01a35105bf80f689f9ee401c9c5ea16",
    "iv": "55b12126b2758724b0f5d967d52180f9d478a5195496ffc40d4e7a683b7a479e",
}


def read_jp_constants(metadata: bytes) -> dict[str, bytes]:
    header = bytes(value ^ 0x66 for value in metadata[:380])
    data_offset, data_size, _ = struct.unpack_from("<III", header, 8 + 8 * 12)
    if data_offset + data_size > len(metadata):
        raise ValueError("invalid metadata default-value data section")
    result: dict[str, bytes] = {}
    for name, relative_offset in FIELD_DATA_INDEX.items():
        if relative_offset + 32 > data_size:
            raise ValueError(f"{name} FieldRVA is outside metadata data section")
        value = metadata[data_offset + relative_offset : data_offset + relative_offset + 32]
        if hashlib.sha256(value).hexdigest() != FIELD_SHA256[name]:
            raise ValueError(f"{name} FieldRVA fingerprint differs from verified Japanese APK")
        result[name] = value
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base_apk", type=Path)
    parser.add_argument("asset_pack_apk", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("artifacts/jp/master-decrypted"))
    args = parser.parse_args()

    with zipfile.ZipFile(args.base_apk) as base:
        metadata = base.read(METADATA_PATH)
    constants = read_jp_constants(metadata)

    results = []
    args.output_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.asset_pack_apk) as assets:
        manifest = json.loads(assets.read(MASTER_ROOT + "MasterManifest.json"))
        for item in manifest["files"]:
            name = item["name"]
            if not name.endswith(".bin") or Path(name).name != name:
                raise ValueError(f"invalid Master filename: {name!r}")
            encrypted = assets.read(MASTER_ROOT + name)
            if len(encrypted) != item["size"] or hashlib.sha256(encrypted).hexdigest() != item["hash"]:
                raise ValueError(f"Master manifest mismatch: {name}")
            plaintext = decrypt_file(encrypted, constants)
            document = json.loads(plaintext)
            if not isinstance(document, dict) or not isinstance(document.get("_allData"), list):
                raise ValueError(f"unexpected Master JSON structure: {name}")
            (args.output_dir / f"{name[:-4]}.json").write_bytes(plaintext)
            results.append({
                "name": name,
                "records": len(document["_allData"]),
                "encrypted_bytes": len(encrypted),
                "json_bytes": len(plaintext),
                "json_sha256": hashlib.sha256(plaintext).hexdigest(),
            })

    summary = {
        "source_base_apk_sha256": file_sha256(args.base_apk),
        "source_asset_pack_apk_sha256": file_sha256(args.asset_pack_apk),
        "metadata_sha256": hashlib.sha256(metadata).hexdigest(),
        "manifest_version": manifest.get("version"),
        "table_count": len(results),
        "total_records": sum(item["records"] for item in results),
        "tables": results,
    }
    (args.output_dir / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"decrypted {len(results)} Master tables ({summary['total_records']} records)")


if __name__ == "__main__":
    main()
