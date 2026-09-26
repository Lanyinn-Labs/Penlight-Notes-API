#!/usr/bin/env python3
"""Decrypt and validate the Master JSON bundled in the global 1.0.1 APK.

Requires py3rijndael (for example: uv run --with py3rijndael python
scripts/decrypt_master.py /path/to/client.apk). Outputs stay under artifacts/.
This is tied to the three verified FieldRVA blobs in this APK's metadata; a
different client build must be analyzed and validated separately.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import struct
import zipfile
from pathlib import Path

try:
    from py3rijndael import Rijndael
except ImportError as exc:
    raise SystemExit("py3rijndael is required: uv run --with py3rijndael python scripts/decrypt_master.py ...") from exc


METADATA_ENTRY = "assets/bin/Data/Managed/Metadata/global-metadata.dat"
MASTER_ROOT = "assets/Master/"
FIELD_DATA_INDEX = {"header": 22984, "key": 22832, "iv": 22744}
FIELD_SHA256 = {
    "header": "6a8a8e599a57e654983fcc8ed280ce9bdc638f0531b99c06c5be10bd6c4c689b",
    "key": "661b3a4e30cbbe6e159c42b2f3c93193f01a35105bf80f689f9ee401c9c5ea16",
    "iv": "55b12126b2758724b0f5d967d52180f9d478a5195496ffc40d4e7a683b7a479e",
}


def read_constants(metadata: bytes) -> dict[str, bytes]:
    # This APK stores its metadata header with a bytewise XOR 0x66 transform.
    header = bytes(value ^ 0x66 for value in metadata[:380])
    data_offset, data_size, _ = struct.unpack_from("<III", header, 8 + 8 * 12)
    if data_offset + data_size > len(metadata):
        raise ValueError("invalid metadata default-value data section")
    values = {}
    for name, index in FIELD_DATA_INDEX.items():
        if index + 32 > data_size:
            raise ValueError(f"{name} FieldRVA is outside metadata data section")
        value = metadata[data_offset + index : data_offset + index + 32]
        if hashlib.sha256(value).hexdigest() != FIELD_SHA256[name]:
            raise ValueError(f"{name} FieldRVA fingerprint differs from global 1.0.1 APK")
        values[name] = value
    return values


def decrypt_file(blob: bytes, constants: dict[str, bytes]) -> bytes:
    if len(blob) < 96 or len(blob) % 32:
        raise ValueError("invalid Rijndael-256 file length")
    if blob[:32] != constants["header"] or blob[32:64] != constants["iv"]:
        raise ValueError("Master file header differs from verified FieldRVA values")

    cipher = Rijndael(constants["key"], block_size=32)
    previous = blob[32:64]
    decrypted = bytearray()
    for start in range(64, len(blob), 32):
        block = blob[start : start + 32]
        decrypted.extend(a ^ b for a, b in zip(cipher.decrypt(block), previous))
        previous = block

    padding = decrypted[-1]
    if padding < 1 or padding > 32 or decrypted[-padding:] != bytes([padding]) * padding:
        raise ValueError("invalid PKCS#7 padding")
    return gzip.decompress(decrypted[:-padding])


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def save_constants(path: Path, constants: dict[str, bytes], apk_sha256: str) -> None:
    """Keep the verified client constants locally, outside version control."""
    path.parent.mkdir(parents=True, exist_ok=True)
    document = {
        "source_apk_sha256": apk_sha256,
        "client_version": "1.0.1",
        "algorithm": "Rijndael-256-CBC-PKCS7",
        "key_hex": constants["key"].hex(),
        "iv_hex": constants["iv"].hex(),
        "header_hex": constants["header"].hex(),
    }
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.fchmod(descriptor, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        json.dump(document, output, indent=2)
        output.write("\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument(
        "--output-dir", type=Path, default=Path("artifacts/global/master-decrypted")
    )
    parser.add_argument("--summary-output", type=Path)
    parser.add_argument(
        "--constants-output",
        type=Path,
        default=Path("artifacts/global/master-crypto-1.0.1.json"),
        help="local, ignored file for the verified Master key and IV",
    )
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)

    results = []
    with zipfile.ZipFile(args.apk) as archive:
        constants = read_constants(archive.read(METADATA_ENTRY))
        manifest = json.loads(archive.read(MASTER_ROOT + "MasterManifest.json"))
        for item in manifest["files"]:
            name = item["name"]
            if not name.endswith(".bin") or Path(name).name != name:
                raise ValueError(f"invalid Master filename: {name!r}")
            encrypted = archive.read(MASTER_ROOT + name)
            if len(encrypted) != item["size"]:
                raise ValueError(f"size mismatch: {name}")
            if hashlib.sha256(encrypted).hexdigest() != item["hash"]:
                raise ValueError(f"manifest SHA-256 mismatch: {name}")

            plaintext = decrypt_file(encrypted, constants)
            document = json.loads(plaintext)
            if not isinstance(document, dict) or not isinstance(document.get("_allData"), list):
                raise ValueError(f"unexpected Master JSON structure: {name}")
            (args.output_dir / f"{name[:-4]}.json").write_bytes(plaintext)
            results.append(
                {
                    "name": name,
                    "records": len(document["_allData"]),
                    "encrypted_bytes": len(encrypted),
                    "json_bytes": len(plaintext),
                    "json_sha256": hashlib.sha256(plaintext).hexdigest(),
                }
            )

    summary = {
        "source_apk_sha256": file_sha256(args.apk),
        "manifest_version": manifest.get("version"),
        "table_count": len(results),
        "total_records": sum(row["records"] for row in results),
        "tables": results,
    }
    save_constants(args.constants_output, constants, summary["source_apk_sha256"])
    rendered = json.dumps(summary, ensure_ascii=False, indent=2) + "\n"
    (args.output_dir / "summary.json").write_text(rendered, encoding="utf-8")
    if args.summary_output:
        args.summary_output.parent.mkdir(parents=True, exist_ok=True)
        args.summary_output.write_text(rendered, encoding="utf-8")
    print(
        f"decrypted {summary['table_count']} Master tables "
        f"({summary['total_records']} records) -> {args.output_dir}"
    )


if __name__ == "__main__":
    main()
