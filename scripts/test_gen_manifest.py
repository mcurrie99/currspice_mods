import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import gen_manifest as gen

class ManifestTests(unittest.TestCase):
    def test_check_is_offline_read_only_and_rejects_missing_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "mods.yaml"
            valid = "mc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://host/f.jar\nserver_mods:\n- url: https://host/a.jar\n  sha256: " + "a" * 64 + "\nuser_mods: []\n"
            src.write_text(valid)
            with patch.object(gen, "SRC", src), patch.object(gen, "sha256", side_effect=AssertionError("network forbidden")):
                self.assertEqual(gen.main(check=True), 0)
                self.assertEqual(src.read_text(), valid)
                src.write_text(valid.replace("  sha256: " + "a" * 64 + "\n", ""))
                with self.assertRaises(ValueError):
                    gen.main(check=True)

    def test_generation_keeps_user_mods_and_writes_valid_mapping(self):
        text = "path:\nmc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://host/f.jar\nserver_mods:\n- https://host/a.jar\nuser_mods:\n- https://host/b.jar\n"
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "mods.yaml"
            src.write_text(text)
            with patch.object(gen, "REPO", Path(tmp)), patch.object(gen, "SRC", src), patch.object(gen, "OUT", src), patch.object(gen, "sha256", return_value=("a" * 64, 1)) as fetch:
                self.assertEqual(gen.main(), 0)
            result = src.read_text()
            self.assertIn("- url: https://host/a.jar", result)
            self.assertIn("user_mods:\n- url: https://host/b.jar", result)
            self.assertEqual(fetch.call_count, 2)

if __name__ == "__main__":
    unittest.main()
