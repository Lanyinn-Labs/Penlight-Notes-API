#!/usr/bin/env python3
"""Import JP 1.0.2 encrypted Master files using the embedded Sirius implementation.

Run with py3rijndael installed (used by the shared metadata reader). Verified
client constants are passed in the child environment and never printed.
"""

import argparse
import os
from pathlib import Path
import subprocess

from decrypt_master_split_apk import read_jp_constants


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("encrypted_directory", type=Path)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("--metadata", type=Path, default=Path("artifacts/jp/global-metadata.dat"))
    parser.add_argument("--executable", type=Path, default=Path("target/debug/penlight-notes-api"))
    args = parser.parse_args()
    constants = read_jp_constants(args.metadata.read_bytes())
    environment = os.environ.copy()
    environment["SIRIUS_MASTER_KEY_HEX"] = constants["key"].hex()
    environment["SIRIUS_MASTER_IV_HEX"] = constants["iv"].hex()
    result = subprocess.run([
        str(args.executable.resolve()), "master-import",
        str(args.encrypted_directory), str(args.output_directory),
    ], env=environment)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
