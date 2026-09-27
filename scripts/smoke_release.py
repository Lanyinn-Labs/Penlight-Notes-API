#!/usr/bin/env python3
"""Verify archive hashes, runtime files and startup without contacting the game."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import secrets
import socket
import subprocess
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile


def verify(archive, expected_target=None):
    archive = Path(archive).resolve()
    checksum = archive.with_name(archive.name + '.sha256').read_text().split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == checksum, 'archive checksum mismatch'
    with tempfile.TemporaryDirectory() as temporary:
        destination = Path(temporary)
        if archive.name.endswith('.zip'):
            with zipfile.ZipFile(archive) as source:
                for name in source.namelist():
                    validate_name(name)
                source.extractall(destination)
        elif archive.name.endswith('.tar.gz'):
            with tarfile.open(archive, 'r:gz') as source:
                for member in source:
                    validate_name(member.name)
                    assert member.isfile() or member.isdir(), 'special archive entry'
                source.extractall(destination, filter='data')
        else:
            raise ValueError('expected a .tar.gz or .zip archive')
        roots = list(destination.iterdir())
        assert len(roots) == 1 and roots[0].is_dir(), 'expected one runtime directory'
        root = roots[0]
        manifest = json.loads((root / 'release-manifest.json').read_text())
        assert manifest['name'] == 'penlight-notes-api'
        if expected_target:
            assert manifest['target'] == expected_target, 'target mismatch'
        files = {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()}
        assert files == set(manifest['files']) | {'release-manifest.json'}, 'file list mismatch'
        for name, digest in manifest['files'].items():
            assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
        for name in files:
            parts = PurePosixPath(name).parts
            assert not set(parts) & {'artifacts', 'secrets', '.local-backups', 'target', '.git', '__pycache__'}
            assert not name.endswith(('.har', '.pcap', '.pcapng', '.local.json', '.local.yaml', '.local.yml'))
            assert not any(p.startswith('.env') and p != '.env.example' for p in parts)
        for name in ['LICENSE', 'THIRD-PARTY-NOTICES.md', 'data/jp-client.json', '.env.example',
                     'vendor/sirius-api-proxy/LICENSE', 'vendor/sirius-api-proxy/LICENSE-protobuf']:
            assert name in files, name
        assert any(name.endswith('.proto') for name in files), 'missing protocol files'
        license_text = (root / 'vendor/sirius-api-proxy/LICENSE').read_text()
        assert 'Haruki Dev Team' in license_text and 'Sirius Project' in license_text
        executable = root / ('penlight-notes-api.exe' if manifest['target'].startswith('windows') else 'penlight-notes-api')
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        environment = {k: v for k, v in os.environ.items()
                       if not k.startswith('PENLIGHT_')}
        token = secrets.token_hex(16)
        environment.update(PENLIGHT_LISTEN=f'127.0.0.1:{port}', PENLIGHT_API_KEY=token,
                           PENLIGHT_JP_ONLINE='true', PENLIGHT_JP_ENDPOINT='https://127.0.0.1:9', RUST_LOG='warn')
        log = root / 'smoke.log'
        with log.open('wb') as output:
            process = subprocess.Popen([str(executable)], cwd=root, env=environment,
                                       stdout=output, stderr=subprocess.STDOUT)
            try:
                def request(path, key=None):
                    headers = {'X-API-Key': key} if key else {}
                    req = urllib.request.Request(f'http://127.0.0.1:{port}' + path, headers=headers)
                    try:
                        with urllib.request.urlopen(req, timeout=2) as response:
                            return response.status, json.load(response)
                    except urllib.error.HTTPError as error:
                        return error.code, json.load(error)
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError('Packaged process exited: ' + log.read_text())
                    try:
                        status, health = request('/health')
                        break
                    except (OSError, urllib.error.URLError):
                        time.sleep(0.1)
                else:
                    raise RuntimeError('Packaged server did not start')
                assert status == 200 and health['upstream_ready'] is False
                assert request('/version')[1]['version'] == manifest['version']
                jp = next(x for x in request('/servers')[1]['servers'] if x['region'] == 'jp')
                assert jp['status'] == 'protocol_configured'
                assert request('/api/jp/master-schema')[0] == 401
                assert request('/api/jp/user/data', 'wrong-key')[0] == 401
                status, schema = request('/api/jp/master-schema', token)
                assert status == 200 and len(schema['entries']) == 235
                assert request('/api/jp/announcements?tab=3', token)[0] == 400
                assert request('/api/global/application', token)[0] == 501
                assert request('/api/jp/master-data', token)[0] == 503
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        print(f"Archive hashes, licenses, protocol startup and API authentication passed: {manifest['target']}")


def validate_name(name):
    path = PurePosixPath(name)
    assert not path.is_absolute() and '..' not in path.parts and '\\' not in name and ':' not in name, name


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--expect-target', choices=['linux-x64', 'macos-arm64', 'windows-x64'])
    args = parser.parse_args()
    verify(args.archive, args.expect_target)
