"""Verify the license metadata and the actual Cargo source-package file lists."""
import json
import os
from pathlib import Path
import subprocess
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.environ.get("SIGNALTTY_VERIFY_TEST_NESTED"),
                 "nested verification exercises a stub Cargo executable")
class LicensePackagingTests(unittest.TestCase):
    def test_package_and_application_licenses_include_incorporated_terms(self):
        result = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--locked", "--offline",
             "--format-version", "1"],
            cwd=ROOT, capture_output=True, text=True, check=True)
        packages = {p["name"]: p for p in json.loads(result.stdout)["packages"]}
        self.assertEqual(packages["signaltty-agent"]["license"],
                         "(MIT OR Apache-2.0) AND Apache-2.0")
        self.assertEqual(packages["signaltty-gui"]["license"],
                         "(MIT OR Apache-2.0) AND MIT")
        app = ET.parse(ROOT / "contrib/dev.signaltty.gui.metainfo.xml")
        self.assertEqual(app.findtext("project_license"),
                         "(MIT OR Apache-2.0) AND Apache-2.0 AND MIT")

    def test_cargo_source_packages_include_applicable_license_bundles(self):
        for name in ["signaltty-agent", "signaltty-gui"]:
            with self.subTest(package=name):
                result = subprocess.run(
                    ["cargo", "package", "-p", name, "--list", "--allow-dirty",
                     "--offline"],
                    cwd=ROOT, capture_output=True, text=True, check=True)
                files = set(result.stdout.splitlines())
                self.assertTrue({"LICENSE-APACHE", "LICENSE-MIT", "NOTICE"} <= files)
                crate = ROOT / "crates" / name
                for license_name in ["LICENSE-APACHE", "LICENSE-MIT"]:
                    self.assertEqual((crate / license_name).read_bytes(),
                                     (ROOT / license_name).read_bytes())
                if name == "signaltty-agent":
                    manifests = {f"screen/{p.name}" for p in (crate / "screen").glob("*.toml")}
                    self.assertTrue(manifests <= files)
                else:
                    self.assertIn("data/style.css", files)
                    self.assertIn("licenses/t3code-MIT.txt", files)
                    self.assertEqual((crate / "licenses/t3code-MIT.txt").read_bytes(),
                                     (ROOT / "licenses/t3code-MIT.txt").read_bytes())
