"""Account exports replace files atomically and never copy HTTP error bodies."""

import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "account_export", ROOT / "scripts/export_account.py"
)
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


class AccountExportTests(unittest.TestCase):
    def test_download_uses_private_header_and_writes_utf8(self):
        document = {
            "schema": "ournotes-account@1",
            "profile": {"profile_id": "123", "name": "名前"},
            "members": [],
            "snapshots": [],
        }
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder, "account.json")
            destination.write_text("old")

            def response(request, timeout):
                self.assertEqual(
                    request.full_url, "http://localhost:8081/api/jp/user/export"
                )
                self.assertEqual(request.get_header("X-api-key"), "PRIVATE_SENTINEL")
                self.assertEqual(timeout, 40)
                return io.BytesIO(json.dumps(document, ensure_ascii=False).encode())

            with patch.object(exporter.urllib.request, "urlopen", side_effect=response):
                exporter.export_account(
                    "http://localhost:8081/", destination, "PRIVATE_SENTINEL"
                )
            self.assertEqual(
                json.loads(destination.read_text(encoding="utf-8")), document
            )
            self.assertNotIn(
                "PRIVATE_SENTINEL", destination.read_text(encoding="utf-8")
            )
            self.assertEqual(list(Path(folder).iterdir()), [destination])

    def test_failure_preserves_previous_export_and_hides_error_body(self):
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder, "account.json")
            destination.write_text("previous")
            error = urllib.error.HTTPError(
                "http://localhost",
                502,
                "PRIVATE_SENTINEL",
                {},
                io.BytesIO(b"PRIVATE_SENTINEL"),
            )
            with patch.object(exporter.urllib.request, "urlopen", side_effect=error):
                with self.assertRaisesRegex(
                    ValueError, "^Account export failed: HTTP 502$"
                ):
                    exporter.export_account("http://localhost", destination, "key")
            with patch.object(
                exporter.urllib.request,
                "urlopen",
                return_value=io.BytesIO(b'{"credential":"PRIVATE_SENTINEL"}'),
            ):
                with self.assertRaisesRegex(
                    ValueError, "Unexpected account export schema"
                ):
                    exporter.export_account("http://localhost", destination, "key")
            self.assertEqual(destination.read_text(), "previous")
            self.assertEqual(list(Path(folder).iterdir()), [destination])


if __name__ == "__main__":
    unittest.main()
