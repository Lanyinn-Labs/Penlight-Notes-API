"""Configuration refresh must preserve the last usable file on every failure."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name != 'nt' and shutil.which('sh'), 'POSIX entrypoint')
class EntrypointTests(unittest.TestCase):
    def run_refresh(self, response, fetch_fails=False, cached_protocol='available'):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / 'client.json'
            if cached_protocol is not None:
                config.write_text(json.dumps({'client_version': '1.0.3', 'protocol_directory': cached_protocol}))
            baseline = root / 'baseline.json'
            baseline.write_text(json.dumps({'client_version': '1.0.3', 'protocol_directory': 'available'}))
            # Map the image's absolute resource path into the test filesystem.
            entrypoint = root / 'entrypoint.sh'
            entrypoint.write_text((ROOT / 'scripts/docker-entrypoint.sh').read_text().replace(
                '/app/data/jp-client.json', str(baseline)))
            source = root / 'response.json'
            source.write_text(response)
            curl = root / 'curl'
            curl.write_text('''#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
    if [ "$1" = '--output' ]; then output=$2; shift; fi
    shift
done
[ "$TEST_FETCH_FAIL" = '0' ] || exit 22
cp "$TEST_RESPONSE" "$output"
''')
            binary = root / 'penlight-notes-api'
            binary.write_text(f'#!{sys.executable}\n' + '''import json,os,sys
if len(sys.argv) == 3 and sys.argv[1] == 'check-client-config':
    try:
        value = json.load(open(sys.argv[2]))
        assert value['protocol_directory'] == 'available'
    except Exception:
        sys.exit(1)
else:
    print(json.load(open(os.environ['PENLIGHT_CLIENT_CONFIG']))['client_version'])
''')
            curl.chmod(0o755)
            binary.chmod(0o755)
            environment = os.environ | {
                'PATH': str(root) + os.pathsep + os.environ['PATH'],
                'PENLIGHT_CLIENT_CONFIG': str(config),
                'PENLIGHT_CLIENT_CONFIG_URL': 'https://example.test/client.json',
                'TEST_RESPONSE': str(source),
                'TEST_FETCH_FAIL': '1' if fetch_fails else '0',
            }
            process = subprocess.run(['sh', str(entrypoint)],
                                     env=environment, capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual(list(root.glob('client.json.*')), [])
            return process.stdout.strip(), process.stderr, json.loads(config.read_text())

    def test_valid_config_replaces_cache_before_program_starts(self):
        output, log, config = self.run_refresh(json.dumps({
            'client_version': '1.0.4', 'protocol_directory': 'available'}))
        self.assertEqual(output, '1.0.4')
        self.assertEqual(config['client_version'], '1.0.4')
        self.assertIn('Updated runtime client configuration', log)

    def test_download_failure_keeps_cache_and_starts_program(self):
        output, log, config = self.run_refresh('unused', fetch_fails=True)
        self.assertEqual(output, '1.0.3')
        self.assertEqual(config['client_version'], '1.0.3')
        self.assertIn('keeping the last valid configuration', log)

    def test_invalid_config_and_missing_protocol_keep_cache(self):
        for response in ['private-sentinel', json.dumps({
                'client_version': '1.0.4', 'protocol_directory': 'unavailable'})]:
            output, log, config = self.run_refresh(response)
            self.assertEqual(output, '1.0.3')
            self.assertEqual(config['client_version'], '1.0.3')
            self.assertNotIn('private-sentinel', log)

    def test_first_offline_start_uses_image_baseline(self):
        output, _, config = self.run_refresh('unused', fetch_fails=True, cached_protocol=None)
        self.assertEqual(output, '1.0.3')
        self.assertEqual(config['protocol_directory'], 'available')

    def test_new_image_replaces_incompatible_cache_before_offline_start(self):
        output, _, config = self.run_refresh('unused', fetch_fails=True, cached_protocol='removed-protocol')
        self.assertEqual(output, '1.0.3')
        self.assertEqual(config['protocol_directory'], 'available')


if __name__ == '__main__':
    unittest.main()
