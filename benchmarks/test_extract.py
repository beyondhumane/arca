import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("extract", Path(__file__).with_name("extract.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class BenchmarkTests(unittest.TestCase):
    def test_payload_determinism_and_bounds(self):
        entries = list(bench.generated_entries(100, True))
        self.assertEqual(entries, list(bench.generated_entries(100, True)))
        self.assertEqual(len(set(path for path, _ in entries)), 100)
        self.assertTrue(all(512 <= len(body) <= 16384 for _, body in entries))

    def test_verification_detects_content_paths_and_empty_directories(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            body = b"test"
            manifest = {"entries": [{"path": "file", "size": 4,
                         "sha256": bench.hashlib.sha256(body).hexdigest()}],
                        "empty_dirs": ["empty"]}
            (root / "file").write_bytes(body)
            (root / "empty").mkdir()
            bench.verify(root, manifest)
            (root / "file").write_bytes(b"fail")
            with self.assertRaises(ValueError):
                bench.verify(root, manifest)
            (root / "file").write_bytes(body)
            (root / "extra").write_bytes(body)
            with self.assertRaises(ValueError):
                bench.verify(root, manifest)
            (root / "extra").unlink()
            (root / "empty").rmdir()
            with self.assertRaises(ValueError):
                bench.verify(root, manifest)

    def test_summary_excludes_warmups_and_reports_dispersion(self):
        rows = [{"case": "c", "variant": "v", "warmup": i == 0,
                 "verified": True, "seconds": value}
                for i, value in enumerate([100, 1, 2, 3, 4, 5, 6])]
        summary = bench.summarize(rows)[0]
        self.assertEqual(summary["n"], 6)
        self.assertEqual(summary["median_s"], 3.5)
        self.assertEqual(summary["mad_s"], 1.5)


if __name__ == "__main__":
    unittest.main()
