#!/usr/bin/env python3
"""Inspect copies of JP 1.0.2 local saves without printing private values.

Requires py3rijndael. Keys are recovered from the verified APK metadata and
never printed. Successful plaintext is saved with mode 0600 under artifacts/.
Matching the Master header alone does not prove that a save uses its key.
"""

import argparse
import gzip
import json
import os
from pathlib import Path

from py3rijndael import Rijndael

from decrypt_master_split_apk import read_jp_constants


def decrypt(blob, constants):
    if len(blob) < 96 or len(blob) % 32:
        raise ValueError("unexpected encrypted file length")
    if blob[:32] != constants["header"]:
        raise ValueError("header differs from verified format")
    # The format stores its CBC IV in the second block.
    previous = blob[32:64]
    cipher = Rijndael(constants["key"], block_size=32)
    plain = bytearray()
    for offset in range(64, len(blob), 32):
        block = blob[offset:offset + 32]
        plain.extend(a ^ b for a, b in zip(cipher.decrypt(block), previous))
        previous = block
    padding = plain[-1]
    if not 1 <= padding <= 32 or plain[-padding:] != bytes([padding]) * padding:
        raise ValueError("verified Master key did not produce valid padding")
    plain = bytes(plain[:-padding])
    if plain.startswith(b"\x1f\x8b"):
        plain = gzip.decompress(plain)
    return plain, json.loads(plain)


def certification_objects(value):
    if isinstance(value, dict):
        normalized = {key.lstrip("_").lower(): item for key, item in value.items()}
        player = normalized.get("playerid")
        credential = normalized.get("authorizationkey")
        if isinstance(player, str) and player and isinstance(credential, str) and credential:
            yield {"player_id": player, "credential": credential}
        for item in value.values():
            yield from certification_objects(item)
    elif isinstance(value, list):
        for item in value:
            yield from certification_objects(item)


def private_write(path, content):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(content)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("save_directory", type=Path)
    parser.add_argument("--metadata", type=Path, default=Path("artifacts/jp/global-metadata.dat"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/jp/local-save-decrypted-private"))
    args = parser.parse_args()
    artifacts = Path("artifacts").resolve()
    destination = args.output.resolve()
    if not destination.is_relative_to(artifacts):
        parser.error("output must stay under the ignored artifacts directory")
    constants = read_jp_constants(args.metadata.read_bytes())
    destination.mkdir(parents=True, mode=0o700, exist_ok=True)
    destination.chmod(0o700)
    for source in sorted(args.save_directory.iterdir()):
        if not source.is_file():
            continue
        try:
            plain, document = decrypt(source.read_bytes(), constants)
        except (ValueError, OSError, UnicodeError):
            print(f"{source.name}: decryption/JSON validation failed")
            continue
        private_write(destination / (source.name + ".json"), plain)
        candidates = list(certification_objects(document))
        for index, candidate in enumerate(candidates):
            private_write(destination / f"{source.name}.credentials-{index}.json",
                          json.dumps(candidate).encode())
        print(f"{source.name}: valid JSON; certification objects: {len(candidates)}")


if __name__ == "__main__":
    main()
