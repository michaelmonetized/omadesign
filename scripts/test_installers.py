import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="omadesign-install-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.package = self.root / "package"
        self.prefix = self.root / "installed app's files"
        self.caller = self.root / "caller"
        self.caller.mkdir()
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.downloads = self.root / "downloads"
        self.downloads.mkdir()
        self.temporary = self.root / "tmp"
        self.temporary.mkdir()
        self.record = self.root / "launch"
        self.curl_log = self.root / "curl.log"
        self.env = os.environ.copy()
        self.env.pop("OMADESIGN_TAG", None)
        self.env.update(
            PATH=f"{self.tools}:{self.env['PATH']}",
            TMPDIR=str(self.temporary),
            OMADESIGN_INSTALL_PREFIX=str(self.prefix),
            TEST_LAUNCH_RECORD=str(self.record),
            TEST_DOWNLOADS=str(self.downloads),
            TEST_CURL_LOG=str(self.curl_log),
        )
        self.env.pop("TEST_CURL_FAIL", None)
        self.env.pop("TEST_APP_EXIT", None)
        for tool in ("update-mime-database", "update-desktop-database"):
            self.write(self.tools / tool, "#!/bin/sh\nexit 0\n", executable=True)
        self.write(
            self.tools / "curl",
            "#!/usr/bin/env python3\n"
            "import os, pathlib, sys\n"
            "args = sys.argv[1:]\n"
            "url = next(a for a in args if a.startswith('https://'))\n"
            "with open(os.environ['TEST_CURL_LOG'], 'a') as log: log.write(url + '\\n')\n"
            "if os.environ.get('TEST_CURL_FAIL'): sys.exit(22)\n"
            "name = 'release.json' if url.endswith('/releases/latest') else url.rsplit('/', 1)[1]\n"
            "data = (pathlib.Path(os.environ['TEST_DOWNLOADS']) / name).read_bytes()\n"
            "if '-o' in args: pathlib.Path(args[args.index('-o') + 1]).write_bytes(data)\n"
            "else: sys.stdout.buffer.write(data)\n",
            executable=True,
        )
        self.make_package()
        self.publish()

    def write(self, path, content, executable=False):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        if executable:
            path.chmod(0o755)

    def binary(self, path, version):
        self.write(
            path,
            "#!/bin/sh\n"
            f"version={shlex.quote(version)}\n"
            "if [ \"${1:-}\" = --version ]; then echo \"omadesign $version\"; exit 0; fi\n"
            "printf '%s\\0' \"$version\" \"$PWD\" \"$@\" > \"$TEST_LAUNCH_RECORD\"\n"
            "exit \"${TEST_APP_EXIT:-0}\"\n",
            executable=True,
        )

    def make_package(self, version="0.6.3"):
        for name in (
            "omadesign.svg", "omadesign-mime.xml", "LICENSE-Phosphor",
            "licenses/libraw/LibRaw-0.22.2.tar.gz", "licenses/libraw/LICENSE.CDDL",
            "licenses/native-notices/NOTICE", "licenses/lua/NOTICE",
            "licenses/ml/ONNXRuntime-ThirdPartyNotices.txt", "licenses/rust/licenses.json",
            "lib/libonnxruntime.so.1", "skills/omadesign-create/SKILL.md",
            "plugins/studio-starter/main.lua", "plugins/studio-starter/orbit.svg",
            "plugins/studio-starter/README.md", "plugins/studio-starter/LICENSE",
            "docs/llms.txt", "docs/MANUAL.md",
            "docs/layout.md", "docs/format-support.md", "docs/cloud.md",
            "docs/plugins.md", "docs/CONTRIBUTING.md",
        ):
            self.write(self.package / name, "fixture\n")
        self.write(self.package / "omadesign.desktop", "[Desktop Entry]\nExec=omadesign %F\n")
        shutil.copy2(REPOSITORY / "scripts/install.sh", self.package / "install.sh")
        self.binary(self.package / "omadesign", version)

    def publish(self, tag="v0.6.3"):
        arch = "aarch64" if platform.machine() in ("aarch64", "arm64") else "x86_64"
        name = f"omadesign-{tag.removeprefix('v')}-{arch}-unknown-linux-gnu"
        archive = self.downloads / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            output.add(self.package, arcname=name)
        self.write(archive.with_name(archive.name + ".sha256"),
                   f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
        self.write(self.downloads / "release.json", json.dumps({"tag_name": tag}))
        return archive

    def local(self, *args):
        return subprocess.run(
            ["sh", str(self.package / "install.sh"), "--prefix", str(self.prefix), *args],
            cwd=self.caller, env=self.env, capture_output=True, text=True,
        )

    def remote(self, *args):
        return subprocess.run(
            ["sh", str(REPOSITORY / "scripts/install-remote.sh"), *args],
            cwd=self.caller, env=self.env, capture_output=True, text=True,
        )

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def launched(self):
        return self.record.read_bytes().split(b"\0")[:-1]

    def test_fresh_local_install_launches_and_forwards_arguments_and_exit_status(self):
        self.env["TEST_APP_EXIT"] = "17"
        result = self.local("--launch", "--", "a file.oma", "--export", "-odd.oma")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertEqual(self.launched(), [b"0.6.3", bytes(self.caller), b"a file.oma", b"--export", b"-odd.oma"])
        self.assertTrue((self.prefix / "share/mime/packages/omadesign.xml").is_file())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_version_ordering_does_not_downgrade(self):
        self.assert_success(self.local())
        cases = (
            ("0.6.3", "0.6.3", False), ("0.6.4", "0.6.3", False),
            ("0.10.0", "0.9.0", False), ("0.6.3", "0.6.3-rc.1", False),
            ("0.6.3-rc.2", "0.6.3", True), ("0.6.3-rc.2", "0.6.3-rc.10", True),
            ("0.6.3-rc.10", "0.6.3-rc.2", False), ("0.6.3-alpha", "0.6.3-beta", True),
            ("0.6.3-alpha.1", "0.6.3-alpha.beta", True),
            ("0.6.3-alpha.beta", "0.6.3-alpha.1", False),
            ("0.6.3-alpha", "0.6.3-alpha.1", True),
            ("0.6.3+build.1", "0.6.3+build.2", False),
        )
        for installed, available, update in cases:
            self.binary(self.package / "omadesign", available)
            self.publish("v" + available)
            for action in (self.local, self.remote):
                with self.subTest(installed=installed, available=available, entry=action.__name__):
                    self.binary(self.prefix / "bin/omadesign", installed)
                    self.assert_success(action("--launch"))
                    self.assertEqual(self.launched()[0].decode(), available if update else installed)

    def test_current_launch_does_not_rewrite_installation(self):
        self.assert_success(self.local())
        target = self.prefix / "bin/omadesign"
        stamp = target.stat().st_mtime_ns
        self.assert_success(self.local("--launch"))
        self.assertEqual(target.stat().st_mtime_ns, stamp)

    def test_launch_repairs_metadata_and_preserves_edited_plugin(self):
        self.assert_success(self.local())
        plugin = self.prefix / "share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
        self.write(plugin, "edited by user\n")
        mime = self.prefix / "share/mime/packages/omadesign.xml"
        mime.unlink()
        self.assert_success(self.local("--launch"))
        self.assertTrue(mime.is_file())
        self.assertEqual(plugin.read_text(), "edited by user\n")

    def test_launch_restores_missing_plugin_files_without_overwriting_edits(self):
        self.write(self.package / "plugins/studio-starter/actions.lua", "stock actions\n")
        self.publish()
        self.assert_success(self.local())
        plugin = self.prefix / "share/omadesign/plugins/org.omadesign.studio-starter"
        self.write(plugin / "actions.lua", "edited by user\n")
        for action in (self.local, self.remote):
            with self.subTest(entry=action.__name__):
                (plugin / "main.lua").unlink()
                self.assert_success(action("--launch"))
                self.assertEqual((plugin / "main.lua").read_text(), "fixture\n")
                self.assertEqual((plugin / "actions.lua").read_text(), "edited by user\n")
                self.assertEqual(self.launched()[0], b"0.6.3")

    def test_remote_does_not_launch_if_older_installer_leaves_setup_incomplete(self):
        self.assert_success(self.local())
        plugin = self.prefix / "share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
        plugin.unlink()
        self.write(self.package / "install.sh", "#!/bin/sh\nexit 0\n", executable=True)
        self.publish()
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("installation is incomplete", result.stderr)
        self.assertFalse(plugin.exists())
        self.assertFalse(self.record.exists())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_launch_repairs_each_starter_asset_and_icon_while_preserving_edited_main(self):
        self.assert_success(self.local())
        plugin = self.prefix / "share/omadesign/plugins/org.omadesign.studio-starter"
        self.write(plugin / "main.lua", "edited by user\n")
        files = [plugin / name for name in ("orbit.svg", "README.md", "LICENSE")]
        files.append(self.prefix / "share/icons/hicolor/scalable/apps/omadesign.svg")
        for file in files:
            for action in (self.local, self.remote):
                with self.subTest(missing=file.name, entry=action.__name__):
                    file.unlink()
                    self.assert_success(action("--launch"))
                    self.assertEqual(file.read_text(), "fixture\n")
                    self.assertEqual((plugin / "main.lua").read_text(), "edited by user\n")
                    self.assertEqual(self.launched()[0], b"0.6.3")

    def test_local_install_and_launch_refuse_store_owned_launcher(self):
        target = self.prefix / "bin/omadesign"
        content = "#!/bin/sh\n# omastore-launcher michaelmonetized/omadesign\nexit 99\n"
        self.write(target, content, executable=True)
        for args in ((), ("--launch",)):
            result = self.local(*args)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("belongs to OmaStore", result.stderr)
            self.assertEqual(target.read_text(), content)
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(target.read_text(), content)
        self.assertFalse(self.curl_log.exists())

    def test_remote_fresh_install_launches_from_caller_directory(self):
        self.assert_success(self.remote("--launch", "--", "a file.oma"))
        self.assertEqual(self.launched(), [b"0.6.3", bytes(self.caller), b"a file.oma"])
        self.assertEqual(len(self.curl_log.read_text().splitlines()), 3)
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_remote_current_launch_only_checks_release(self):
        self.assert_success(self.local())
        stamp = (self.prefix / "bin/omadesign").stat().st_mtime_ns
        self.assert_success(self.remote("--launch"))
        self.assertEqual(self.curl_log.read_text().splitlines(), ["https://api.github.com/repos/michaelmonetized/omadesign/releases/latest"])
        self.assertEqual((self.prefix / "bin/omadesign").stat().st_mtime_ns, stamp)

    def test_remote_upgrades_older_version(self):
        self.assert_success(self.local())
        self.binary(self.prefix / "bin/omadesign", "0.6.2")
        self.assert_success(self.remote("--launch"))
        self.assertEqual(self.launched()[0], b"0.6.3")
        self.assertEqual(len(self.curl_log.read_text().splitlines()), 3)

    def test_remote_does_not_downgrade_newer_version(self):
        self.assert_success(self.local())
        self.binary(self.prefix / "bin/omadesign", "0.6.4")
        self.assert_success(self.remote("--launch"))
        self.assertEqual(self.launched()[0], b"0.6.4")
        self.assertEqual(len(self.curl_log.read_text().splitlines()), 1)

    def test_repair_never_replaces_newer_binary_with_older_package(self):
        self.assert_success(self.local())
        target = self.prefix / "bin/omadesign"
        self.binary(target, "0.6.4")
        previous = target.read_bytes()
        (self.prefix / "share/mime/packages/omadesign.xml").unlink()
        for action in (self.local, self.remote):
            result = action("--launch")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("matching or newer package", result.stderr)
            self.assertEqual(target.read_bytes(), previous)
            self.assertFalse(self.record.exists())

    def test_offline_remote_launch_uses_existing_complete_installation(self):
        self.assert_success(self.local())
        self.env["TEST_CURL_FAIL"] = "1"
        self.assert_success(self.remote("--launch"))
        self.assertEqual(self.launched()[0], b"0.6.3")

    def test_offline_remote_launch_cannot_install_missing_app(self):
        self.env["TEST_CURL_FAIL"] = "1"
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.prefix / "bin/omadesign").exists())
        self.assertFalse(self.record.exists())

    def test_bad_checksum_preserves_old_binary_without_launching(self):
        self.assert_success(self.local())
        self.binary(self.prefix / "bin/omadesign", "0.6.2")
        target = self.prefix / "bin/omadesign"
        previous = target.read_bytes()
        archive = self.publish()
        self.write(archive.with_name(archive.name + ".sha256"), f"{'0' * 64}  {archive.name}\n")
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(target.read_bytes(), previous)
        self.assertFalse(self.record.exists())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_wrong_release_binary_version_is_not_launched(self):
        self.binary(self.package / "omadesign", "0.6.2")
        self.publish()
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected 0.6.3", result.stderr)
        self.assertFalse(self.record.exists())
        self.assertFalse((self.prefix / "bin/omadesign").exists())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_wrong_release_binary_version_preserves_existing_installation(self):
        self.assert_success(self.local())
        target = self.prefix / "bin/omadesign"
        self.binary(target, "0.6.2")
        previous = target.read_bytes()
        plugin = self.prefix / "share/omadesign/plugins/org.omadesign.studio-starter/main.lua"
        self.write(plugin, "edited by user\n")
        self.binary(self.package / "omadesign", "0.6.1")
        self.publish()
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected 0.6.3", result.stderr)
        self.assertEqual(target.read_bytes(), previous)
        self.assertEqual(plugin.read_text(), "edited by user\n")
        self.assertFalse(self.record.exists())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_relative_remote_prefix_is_relative_to_caller(self):
        self.env["OMADESIGN_INSTALL_PREFIX"] = "relative prefix"
        self.assert_success(self.remote("--launch"))
        self.assertTrue((self.caller / "relative prefix/bin/omadesign").is_file())
        self.assertEqual(self.launched()[1], bytes(self.caller))

    def test_remote_without_launch_still_reinstalls(self):
        self.assert_success(self.local())
        target = self.prefix / "bin/omadesign"
        self.binary(target, "0.6.4")
        self.assert_success(self.remote())
        self.assertIn("0.6.3", target.read_text())
        self.assertFalse(self.record.exists())

    def test_invalid_release_version_is_rejected(self):
        self.assert_success(self.local())
        self.write(self.downloads / "release.json", json.dumps({"tag_name": "release"}))
        result = self.remote("--launch")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid release version", result.stderr)
        self.assertFalse(self.record.exists())

    def test_published_installer_matches_source(self):
        self.assertEqual((REPOSITORY / "site/public/install").read_bytes(),
                         (REPOSITORY / "scripts/install-remote.sh").read_bytes())


if __name__ == "__main__":
    unittest.main()
