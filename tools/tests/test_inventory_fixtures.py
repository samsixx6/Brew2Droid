"""Prova a ferramenta real sem depender dos jogos privados."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


PROJECT = Path(__file__).resolve().parents[2]
SCRIPT = PROJECT / "tools" / "inventory_fixtures.py"
ABC_SHA256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"


class InventoryTests(unittest.TestCase):
    def setUp(self):
        scratch = PROJECT / ".superpowers" / "inventory-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.collection = self.base / "collection"
        self.module = self.collection / "mod" / "Toy" / "toy.mod"
        self.module.parent.mkdir(parents=True)
        self.module.write_bytes(b"abc")
        (self.module.parent / "data.bar").write_bytes(b"xyz")
        self.mif = self.collection / "mif" / "Toy.mif"
        self.mif.parent.mkdir()
        self.mif.write_bytes(b"\x11\x00\x01\x00")
        self.catalog = self.base / "catalog.json"
        self.entry = {
            "id": "toy",
            "module": "mod/Toy/toy.mod",
            "mif": "mif/Toy.mif",
            "module_sha256": ABC_SHA256,
        }
        self.write_catalog()

    def write_catalog(self):
        self.catalog.write_text(json.dumps({"schema_version": 1, "games": [self.entry]}))

    def run_tool(self, output):
        return subprocess.run(
            [sys.executable, "-B", str(SCRIPT), "--root", str(self.collection),
             "--catalog", str(self.catalog), "--output", str(output)],
            capture_output=True, text=True, check=False,
        )

    def test_records_actual_bytes_and_relative_paths(self):
        output = self.base / "reports" / "inventory.json"
        result = self.run_tool(output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        game = report["games"][0]
        files = {item["path"]: item for item in game["files"]}
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(game["file_count"], 3)
        self.assertEqual(game["total_size_bytes"], 10)
        self.assertEqual(set(files), {"mod/Toy/toy.mod", "mod/Toy/data.bar", "mif/Toy.mif"})
        self.assertEqual(files["mod/Toy/toy.mod"]["sha256"], ABC_SHA256)
        self.assertEqual(files["mod/Toy/toy.mod"]["size_bytes"], 3)
        self.assertEqual(files["mif/Toy.mif"]["role"], "mif")
        self.assertEqual(self.module.read_bytes(), b"abc")

    def test_rejects_output_inside_source_collection(self):
        output = self.collection / "new-report.json"
        result = self.run_tool(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("outside the source collection", result.stderr)
        self.assertFalse(output.exists())
        self.assertEqual(self.module.read_bytes(), b"abc")

    def test_rejects_changed_module_before_writing_report(self):
        self.module.write_bytes(b"abd")
        output = self.base / "inventory.json"
        result = self.run_tool(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("hash mismatch", result.stderr)
        self.assertFalse(output.exists())

    def test_rejects_catalog_path_outside_collection(self):
        (self.base / "outside.mod").write_bytes(b"abc")
        self.entry["module"] = "../outside.mod"
        self.write_catalog()
        output = self.base / "inventory.json"
        result = self.run_tool(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("outside the source collection", result.stderr)
        self.assertFalse(output.exists())

    def test_missing_mif_does_not_write_partial_report(self):
        self.mif.unlink()
        output = self.base / "inventory.json"
        result = self.run_tool(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing file", result.stderr)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
