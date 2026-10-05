"""Local documents must never enter a runtime archive, even when present on disk."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_packager', ROOT / 'scripts/package_release.py')
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class ReleasePackagingTests(unittest.TestCase):
    def test_runtime_archives_exclude_local_docs_and_keep_public_instructions(self):
        for target in ['linux-x64', 'windows-x64']:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / 'Cargo.toml').write_text('[package]\nname = "penlight-notes-api"\nversion = "0.3.0"\n')
                for name in packager.PUBLIC_FILES:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('public runtime documentation\n')
                for name in packager.TOOLS:
                    path = root / 'scripts' / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('public runtime tool\n')
                for tree in packager.PUBLIC_TREES:
                    path = root / tree / 'public.txt'
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('public upstream file\n')
                private = ['CONTRIBUTING.md', '.gitmessage', 'docs/releases.md', 'docs/roadmap.md',
                           'docs/android-save-extraction.local.md', 'docs/client-maintenance.local.md',
                           'docs/unlisted-private-note.md']
                for name in private:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('LOCAL_ONLY_SENTINEL\n')
                binary = root / 'test-binary'
                binary.write_bytes(b'test executable')
                with patch.object(packager, 'ROOT', root), contextlib.redirect_stdout(io.StringIO()):
                    archive = packager.package(target, binary)
                if target.startswith('windows'):
                    with zipfile.ZipFile(archive) as source:
                        contents = {name.split('/', 1)[1]: source.read(name)
                                    for name in source.namelist()}
                else:
                    with tarfile.open(archive) as source:
                        contents = {member.name.split('/', 1)[1]: source.extractfile(member).read()
                                    for member in source.getmembers() if member.isfile()}
                for name in private:
                    self.assertNotIn(name, contents)
                self.assertFalse(any(b'LOCAL_ONLY_SENTINEL' in content for content in contents.values()))
                for name in ['docs/downloads.md', 'docs/api.md', 'docs/configuration.md',
                             'docs/account-setup.md', 'docs/updates.md', 'docs/upstream-attribution.md',
                             'LICENSE', 'vendor/sirius-api-proxy/LICENSE']:
                    self.assertIn(name, contents)
                manifest = json.loads(contents['release-manifest.json'])
                self.assertEqual(set(manifest['files']), set(contents) - {'release-manifest.json'})


if __name__ == '__main__':
    unittest.main()
