"""Version detection must verify app identity and metadata instead of guessing numbers."""
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('android_version', ROOT / 'scripts/check_android_version.py')
monitor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(monitor)


def listing(version='1.0.3', package=monitor.PACKAGE):
    metadata = {'@type': 'SoftwareApplication', 'url': f'https://play.google.com/store/apps/details?id={package}'}
    app = [None] * 141
    app[140] = [[[version]]]
    data = [None, [None, None, app]]
    return (f'<script type="application/ld+json">{json.dumps(metadata)}</script>'
            f"<script>AF_initDataCallback({{key: 'ds:5', hash: '11', data:{json.dumps(data)}, sideChannel: {{}}}});</script>")


class AndroidVersionTests(unittest.TestCase):
    def test_known_metadata_and_additive_structured_metadata(self):
        self.assertEqual(monitor.parse_version(listing()), '1.0.3')
        page = listing().replace('"@type": "SoftwareApplication"', '"softwareVersion": "1.0.4", "@type": "SoftwareApplication"')
        self.assertEqual(monitor.parse_version(page), '1.0.4')

    def test_wrong_application_and_changed_metadata_fail(self):
        for page in [listing(package='example.other'), listing().replace('ds:5', 'ds:6'),
                     listing(version='Varies with device'), '<p>latest 9.9.9</p>']:
            with self.assertRaises(ValueError):
                monitor.parse_version(page)

    def test_numeric_versions_and_store_rollbacks(self):
        self.assertEqual(monitor.report('1.0.9', '1.0.10')['status'], 'update_available')
        self.assertEqual(monitor.report('1.0.3', '1.0.3')['status'], 'current')
        self.assertEqual(monitor.report('1.0.3', '1.0.1')['status'], 'store_older')
        with self.assertRaises(ValueError):
            monitor.report('1.0.3', '1.0.4; shell')

    def test_cli_reports_update_without_changing_client_config(self):
        import subprocess
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            version = root / 'version'
            original = json.dumps({'client_version':'1.0.3', 'master':{'key_hex':'preserved-key', 'iv_hex':'preserved-iv'}, 'cdn_username':'preserved-user', 'cdn_password':'preserved-password'})
            version.write_text(original)
            page = root / 'page.html'
            page.write_text(listing(version='1.0.4'))
            process = subprocess.run([sys.executable, str(ROOT / 'scripts/check_android_version.py'),
                '--client-config', str(version), '--html-file', str(page)],
                capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual(json.loads(process.stdout)['status'], 'update_available')
            self.assertEqual(json.loads(process.stdout)['store_version'], '1.0.4')
            self.assertEqual(version.read_text(), original)


if __name__ == '__main__':
    unittest.main()
