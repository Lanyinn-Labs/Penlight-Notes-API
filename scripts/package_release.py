#!/usr/bin/env python3
"""Package a complete runtime using an explicit public-file allowlist."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_FILES = ['README.md', 'CHANGELOG.md', 'LICENSE', 'THIRD-PARTY-NOTICES.md', '.env.example',
                'docs/account-setup.md', 'docs/api.md', 'docs/configuration.md', 'docs/downloads.md',
                'docs/updates.md', 'docs/upstream-attribution.md',
                'data/jp-client.json', 'vendor/sirius-api-proxy/LICENSE',
                'vendor/sirius-api-proxy/LICENSE-protobuf', 'vendor/sirius-api-proxy/UPSTREAM.json']
PUBLIC_TREES = ['vendor/sirius-api-proxy/protocol', 'vendor/sirius-api-proxy/docs']
TOOLS = ['check_android_version.py', 'import_jp_master.py', 'inspect_jp_local_save.py', 'decrypt_master.py',
         'decrypt_master_split_apk.py', 'README.md']


def package(target, binary=None):
    metadata = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
    name, version = metadata['name'], metadata['version']
    executable = name + ('.exe' if target.startswith('windows') else '')
    binary = Path(binary) if binary else ROOT / 'target/release' / executable
    if not binary.is_file() or binary.is_symlink():
        raise ValueError(f'Missing regular executable: {binary}')
    dist = ROOT / 'dist'
    dist.mkdir(exist_ok=True)
    stem = f'{name}-{version}-{target}'
    with tempfile.TemporaryDirectory() as temporary:
        stage = Path(temporary) / stem
        stage.mkdir()
        shutil.copy2(binary, stage / executable)
        sources = [ROOT / f for f in PUBLIC_FILES]
        sources += [ROOT / 'scripts' / f for f in TOOLS]
        for tree in PUBLIC_TREES:
            sources.extend(p for p in (ROOT / tree).rglob('*') if p.is_file())
        for source in sources:
            if source.is_symlink():
                raise ValueError(f'Release input is a symlink: {source}')
            relative = source.relative_to(ROOT)
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        manifest = {'name': name, 'version': version, 'target': target, 'files': {
            p.relative_to(stage).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(stage.rglob('*')) if p.is_file()}}
        (stage / 'release-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        archive = dist / (stem + ('.zip' if target.startswith('windows') else '.tar.gz'))
        if target.startswith('windows'):
            with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
                for p in sorted(stage.rglob('*')):
                    if p.is_file():
                        output.write(p, p.relative_to(stage.parent))
        else:
            with tarfile.open(archive, 'w:gz') as output:
                output.add(stage, arcname=stem)
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        archive.with_name(archive.name + '.sha256').write_text(f'{checksum}  {archive.name}\n', newline='\n')
        print(archive)
        return archive


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True, choices=['linux-x64', 'macos-arm64', 'windows-x64'])
    parser.add_argument('--binary', type=Path, help='Override executable path for local packaging checks')
    args = parser.parse_args()
    package(args.target, args.binary)
