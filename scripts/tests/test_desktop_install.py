"""Exercise desktop installation in a disposable repository with a stub build."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DesktopInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="signaltty-desktop-")
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / "repo"
        shutil.copytree(ROOT / "contrib", self.repo / "contrib")
        icons = Path("crates/signaltty-gui/data/icons/scalable/apps")
        shutil.copytree(ROOT / icons, self.repo / icons)
        binary = self.repo / "target/release/signaltty-gui"
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        tools = Path(self.temp.name) / "tools"
        tools.mkdir()
        cargo = tools / "cargo"
        cargo.write_text('#!/bin/sh\ntouch "$BUILD_MARKER"\n')
        cargo.chmod(0o755)
        self.marker = Path(self.temp.name) / "built"
        self.env = dict(os.environ, PATH=f"{tools}:/usr/bin:/bin", BUILD_MARKER=str(self.marker))

    def run_install(self, prefix):
        return subprocess.run(["sh", str(self.repo / "contrib/install-desktop.sh"), "install"],
                              env=dict(self.env, SIGNALTTY_PREFIX=str(prefix)),
                              capture_output=True, text=True)

    def test_absolute_launcher_handles_spaces_and_preserves_template(self):
        prefix = Path(self.temp.name) / "prefix with spaces"
        result = self.run_install(prefix)
        self.assertEqual(result.returncode, 0, result.stderr)
        entry = prefix / "share/applications/dev.signaltty.gui.desktop"
        lines = entry.read_text().splitlines()
        self.assertIn(f'Exec="{prefix}/bin/signaltty-gui"', lines)
        source = (self.repo / "contrib/dev.signaltty.gui.desktop").read_text().splitlines()
        self.assertEqual([line for line in lines if not line.startswith("Exec=")],
                         [line for line in source if not line.startswith("Exec=")])
        self.assertEqual(entry.stat().st_mode & 0o777, 0o644)
        self.assertTrue(self.marker.exists())
        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(entry)], check=True)
        # Run the quoted executable with no installation bin directory on PATH.
        subprocess.run([str(prefix / "bin/signaltty-gui")], env=self.env, check=True)

    def test_invalid_prefix_fails_before_build(self):
        for suffix in ['"', "`", "$", "\\", "%", "\n", "\t", "\r"]:
            with self.subTest(suffix=suffix):
                result = self.run_install(Path(self.temp.name) / ("bad" + suffix))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertFalse(self.marker.exists())
        result = self.run_install("relative-prefix")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertFalse(self.marker.exists())
