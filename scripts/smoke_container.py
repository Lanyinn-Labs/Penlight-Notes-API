#!/usr/bin/env python3
"""Check non-root container startup and embedded protocols with networking disabled."""
import argparse
import json
from pathlib import Path
import secrets
import subprocess
import time
import tomllib


def docker(*arguments, check=True):
    result = subprocess.run(['docker', *arguments], capture_output=True, text=True, timeout=30)
    if check and result.returncode:
        raise RuntimeError(result.stderr)
    return result


def verify(image):
    image_id = json.loads(docker('image', 'inspect', image).stdout)[0]['Id']
    expected_version = tomllib.loads(Path('Cargo.toml').read_text())['package']['version']
    assert docker('run', '--rm', '--network', 'none', '--entrypoint', 'id', image, '-u').stdout.strip() == '10001'
    docker('run', '--rm', '--network', 'none', '--entrypoint', 'sh', image, '-c',
           'test -f /usr/share/doc/penlight-notes-api/LICENSE && '
           'test -f /usr/share/doc/penlight-notes-api/SIRIUS-LICENSE && '
           'test -f /usr/share/doc/penlight-notes-api/PROTOBUF-LICENSE')
    token = secrets.token_hex(16)
    container = docker('run', '--detach', '--network', 'none',
                       '-e', 'PENLIGHT_CLIENT_CONFIG_URL=',
                       '-e', 'PENLIGHT_JP_ONLINE=true',
                       '-e', 'PENLIGHT_JP_ENDPOINT=https://127.0.0.1:9',
                       '-e', f'PENLIGHT_API_KEY={token}', image).stdout.strip()
    try:
        def request(path, key=None):
            args = ['exec', container, 'wget', '-S', '-O', '-']
            if key:
                args += ['--header', 'X-API-Key: ' + key]
            args += ['http://127.0.0.1:8081' + path]
            return docker(*args, check=False)
        for _ in range(100):
            response = request('/health')
            if response.returncode == 0:
                break
            if docker('inspect', '--format', '{{.State.Running}}', container).stdout.strip() != 'true':
                raise RuntimeError('Container exited: ' + docker('logs', container).stdout)
            time.sleep(0.1)
        else:
            raise RuntimeError('Container did not start')
        assert json.loads(response.stdout)['upstream_ready'] is False
        assert json.loads(request('/version').stdout)['version'] == expected_version
        jp = next(x for x in json.loads(request('/servers').stdout)['servers'] if x['region'] == 'jp')
        assert jp['status'] == 'protocol_configured'
        assert '401' in request('/api/jp/master-schema').stderr
        assert '401' in request('/api/jp/user/data', 'wrong-key').stderr
        response = request('/api/jp/master-schema', token)
        assert response.returncode == 0 and len(json.loads(response.stdout)['entries']) == 235
        assert '400' in request('/api/jp/announcements?tab=3', token).stderr
        assert '503' in request('/api/jp/master-data', token).stderr
    finally:
        docker('rm', '--force', container)
    print(f'Non-root container, licenses, protocols and authentication passed with network disabled: {image_id}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image')
    verify(parser.parse_args().image)
