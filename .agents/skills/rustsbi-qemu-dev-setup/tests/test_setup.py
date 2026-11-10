#!/usr/bin/env python3
# SPDX-License-Identifier: MulanPSL-2.0

"""Offline regression tests for setup write boundaries and command interfaces."""

import ast
import hashlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest


PACKAGE = Path(__file__).resolve().parents[1]
SCRIPTS = PACKAGE / "scripts"

COMMAND = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
root = pathlib.Path(os.environ['SETUP_TEST_ROOT'])
if name in ('apt', 'apt-get', 'pacman', 'dnf', 'zypper', 'apk', 'sudo', 'curl'):
    (root / 'forbidden-command').write_text(name)
    sys.exit(97)
if name in ('cargo', 'rustc', 'rustup', 'rust-objcopy'):
    if os.environ.get('SETUP_TEST_DENY_PROXIES'):
        (root / 'proxy-executed').write_text(name)
        sys.exit(98)
if name == 'qemu-system-riscv64':
    print('QEMU emulator version 10.2.1')
elif name == 'cargo':
    with (root / 'cargo-calls').open('a') as log:
        log.write(' '.join(sys.argv[1:]) + '\n')
    if sys.argv[1:] == ['prototyper', 'build']:
        release = pathlib.Path.cwd() / 'target/riscv64gc-unknown-none-elf/release'
        release.mkdir(parents=True, exist_ok=True)
        (release / 'rustsbi-prototyper-dynamic.elf').write_text('ELF')
        if not os.environ.get('SETUP_TEST_MISSING_BIN'):
            (release / 'rustsbi-prototyper-dynamic.bin').write_text('BIN')
    print('cargo fixture success')
elif name == 'make':
    args = sys.argv[1:]
    if 'install' in args:
        prefix = pathlib.Path(json.loads(pathlib.Path('configuration.json').read_text())['prefix'])
        stage = pathlib.Path(next(value.split('=', 1)[1] for value in args if value.startswith('DESTDIR=')))
        payload = stage / str(prefix).lstrip('/')
        for item in ('bin/qemu-system-riscv64', 'include/fdt.h', 'lib/libfdt.a', 'share/qemu/trace-events-all'):
            path = payload / item
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('#!/bin/sh\necho "QEMU emulator version 10.2.1"\n' if item.startswith('bin/') else 'fixture')
            if item.startswith('bin/'):
                path.chmod(0o755)
        if os.environ.get('SETUP_TEST_INSTALL_ESCAPE'):
            escaped = stage / 'usr/local/bin/unapproved-helper'
            escaped.parent.mkdir(parents=True, exist_ok=True)
            escaped.write_text('must not be published')
elif name == 'git' and sys.argv[1] == 'clone':
    target = pathlib.Path(sys.argv[-1])
    for item in ('Cargo.toml', 'rust-toolchain.toml', 'firmware/prototyper/Cargo.toml', 'library/rustsbi/Cargo.toml'):
        path = target / item
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('fixture')
    if os.environ.get('SETUP_TEST_RACE_TARGET'):
        target = pathlib.Path(os.environ['SETUP_TEST_RACE_TARGET'])
        target.mkdir()
        (target / 'keep.txt').write_text('concurrent writer')
'''

CONFIGURE = r'''#!/usr/bin/env python3
import json, pathlib, sys
prefix = next(value.split('=', 1)[1] for value in sys.argv[1:] if value.startswith('--prefix='))
required = ('--disable-tools', '--disable-guest-agent', '--disable-plugins', '--disable-install-blobs')
assert all(value in sys.argv for value in required)
assert '--localstatedir=' + prefix + '/var' in sys.argv
assert '--sysconfdir=' + prefix + '/etc' in sys.argv
pathlib.Path('configuration.json').write_text(json.dumps({'prefix': prefix}))
'''


def workspace(path):
    for item in ("Cargo.toml", "rust-toolchain.toml", "firmware/prototyper/Cargo.toml",
                 "library/rustsbi/Cargo.toml"):
        target = path / item
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('[toolchain]\nchannel = "nightly-2099-01-01"\n')


def snapshot(root):
    entries = {}
    for path in root.rglob("*"):
        if path.is_symlink():
            value = ("symlink", os.readlink(path))
        elif path.is_file():
            value = ("file", hashlib.sha256(path.read_bytes()).hexdigest())
        else:
            value = ("directory",)
        entries[str(path.relative_to(root))] = value
    return entries


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rustsbi-setup-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.commands = self.root / "commands"
        self.commands.mkdir()
        for name in ("apt", "apt-get", "pacman", "dnf", "zypper", "apk", "sudo", "curl",
                     "cargo", "rustc", "rustup", "rust-objcopy", "qemu-system-riscv64",
                     "git", "make", "ninja", "meson", "pkg-config", "cc", "c++"):
            path = self.commands / name
            path.write_text(COMMAND)
            path.chmod(0o755)
        self.env = os.environ.copy()
        self.env.update({"PATH": f"{self.commands}:/usr/bin:/bin",
                         "HOME": str(self.root / "home"),
                         "CARGO_HOME": str(self.root / "cargo-home"),
                         "RUSTUP_HOME": str(self.root / "rustup-home"),
                         "TMPDIR": str(self.root), "SETUP_TEST_ROOT": str(self.root)})
        self.repo = self.root / "repo"
        workspace(self.repo)

    def tearDown(self):
        self.assertFalse((self.root / "forbidden-command").exists(),
                         "setup invoked a system installer or unexpected download")

    def run_script(self, script, *args, env=None):
        return subprocess.run(["bash", str(SCRIPTS / script), *map(str, args)],
                              cwd=self.root, env=env or self.env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)

    def files(self, *args):
        return subprocess.run([sys.executable, "-B", str(SCRIPTS / "files.py"), *map(str, args)],
                              cwd=self.root, env=self.env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)

    def require_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_preflight_does_not_execute_rust_proxies_or_write_files(self):
        self.env["SETUP_TEST_DENY_PROXIES"] = "1"
        before = snapshot(self.root)
        result = self.run_script("preflight.sh", "--repo-dir", self.repo)
        self.require_success(result)
        self.assertEqual(before, snapshot(self.root))
        self.assertIn("not executed", result.stdout)

    def test_plan_does_not_write_files(self):
        self.env["SETUP_TEST_DENY_PROXIES"] = "1"
        before = snapshot(self.root)
        result = self.run_script("setup.sh", "--repo-dir", self.root / "new",
                                 "--local-copy", self.repo, "--plan")
        self.require_success(result)
        self.assertEqual(before, snapshot(self.root))

    def test_existing_directory_preserves_its_files(self):
        target = self.root / "existing"
        target.mkdir()
        (target / "Cargo.toml").write_text("unrelated project")
        (target / "keep.txt").write_text("user data")
        result = self.run_script("setup.sh", "--repo-dir", target,
                                 "--local-copy", self.repo, "--yes")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((target / "keep.txt").read_text(), "user data")
        self.assertEqual((target / "Cargo.toml").read_text(), "unrelated project")
        self.assertFalse((self.root / "cargo-calls").exists())

    def test_empty_directory_and_symlink_are_refused(self):
        empty = self.root / "empty"
        empty.mkdir()
        alias = self.root / "alias"
        alias.symlink_to(self.repo, target_is_directory=True)
        for target in (empty, alias):
            with self.subTest(target=target):
                result = self.files("prepare-source", target, self.repo)
                self.assertNotEqual(result.returncode, 0)
        self.assertTrue(alias.is_symlink())
        self.assertEqual(list(empty.iterdir()), [])

    def test_racing_destination_is_not_replaced(self):
        target = self.root / "racing"
        self.env["SETUP_TEST_RACE_TARGET"] = str(target)
        result = self.files("prepare-source", target)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((target / "keep.txt").read_text(), "concurrent writer")

    def test_directory_source_is_published(self):
        target = self.root / "copy"
        self.require_success(self.files("prepare-source", target, self.repo))
        self.assertEqual((target / "Cargo.toml").read_text(), (self.repo / "Cargo.toml").read_text())

    def test_flat_and_wrapped_archives(self):
        for wrapper in ("", "rustsbi"):
            with self.subTest(wrapper=wrapper):
                archive = self.root / f"source-{wrapper or 'flat'}.tar"
                with tarfile.open(archive, "w") as output:
                    for path in self.repo.rglob("*"):
                        output.add(path, arcname=str(Path(wrapper) / path.relative_to(self.repo)),
                                   recursive=False)
                target = self.root / f"unpacked-{wrapper or 'flat'}"
                self.require_success(self.files("prepare-source", target, archive))
                self.assertTrue((target / "Cargo.toml").is_file())

    def test_unsafe_archive_does_not_escape_staging(self):
        archive = self.root / "unsafe.tar"
        with tarfile.open(archive, "w") as output:
            member = tarfile.TarInfo("../../escape")
            member.size = 4
            output.addfile(member, io.BytesIO(b"data"))
        result = self.files("prepare-source", self.root / "unsafe-target", archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "escape").exists())
        self.assertFalse((self.root / "unsafe-target").exists())

    def stage(self):
        stage = self.root / "stage"
        prefix = self.root / "private" / "qemu"
        payload = stage / str(prefix).lstrip("/")
        for item in ("bin/qemu-system-riscv64", "include/fdt.h", "lib/libfdt.a"):
            path = payload / item
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture")
        return stage, prefix

    def test_install_manifest_covers_headers_and_libraries(self):
        stage, prefix = self.stage()
        (stage / str(prefix).lstrip("/") / "var/run").mkdir(parents=True)
        self.require_success(self.files("install-prefix", stage, prefix))
        manifest = (prefix / "install-manifest.txt").read_text().splitlines()
        self.assertEqual(manifest, ["bin/", "bin/qemu-system-riscv64", "include/", "include/fdt.h",
                                    "lib/", "lib/libfdt.a", "var/", "var/run/", "install-manifest.txt"])

    def test_installation_outside_prefix_is_refused(self):
        stage, prefix = self.stage()
        escaped = stage / "usr/local/bin/foreign-tool"
        escaped.parent.mkdir(parents=True)
        escaped.write_text("not approved")
        result = self.files("install-prefix", stage, prefix)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(prefix.exists())

    def test_existing_prefix_is_preserved(self):
        stage, prefix = self.stage()
        prefix.mkdir(parents=True)
        (prefix / "keep.txt").write_text("keep")
        result = self.files("install-prefix", stage, prefix)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((prefix / "keep.txt").read_text(), "keep")

    def test_empty_directory_outside_prefix_is_refused(self):
        stage, prefix = self.stage()
        (stage / "var/unapproved").mkdir(parents=True)
        result = self.files("install-prefix", stage, prefix)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(prefix.exists())

    def test_symlinked_staged_prefix_is_refused(self):
        stage, prefix = self.stage()
        payload = stage / str(prefix).lstrip("/")
        external = self.root / "external"
        payload.rename(external)
        payload.symlink_to(external, target_is_directory=True)
        result = self.files("install-prefix", stage, prefix)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(prefix.exists())
        self.assertFalse((external / "install-manifest.txt").exists())

    def test_setup_reuses_qemu_without_package_managers(self):
        result = self.run_script("setup.sh", "--repo-dir", self.repo,
                                 "--qemu-from", "distro", "--yes")
        self.require_success(result)
        self.assertIn("boot_verified: 1", result.stdout)

    def test_no_test_reports_unverified_build(self):
        result = self.run_script("setup.sh", "--repo-dir", self.repo, "--yes", "--no-test")
        self.require_success(result)
        self.assertIn("boot_verified: 0", result.stdout)
        self.assertNotIn("prototyper test", (self.root / "cargo-calls").read_text())

    def test_missing_binary_is_a_failure(self):
        self.env["SETUP_TEST_MISSING_BIN"] = "1"
        result = self.run_script("setup.sh", "--repo-dir", self.repo, "--yes", "--no-test")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("ENVIRONMENT UP", result.stdout)

    def qemu_archive(self):
        archive = self.root / "qemu.tar"
        with tarfile.open(archive, "w") as output:
            for name, content in (("qemu/configure", CONFIGURE), ("qemu/meson.build", "fixture")):
                member = tarfile.TarInfo(name)
                member.mode = 0o755
                data = content.encode()
                member.size = len(data)
                output.addfile(member, io.BytesIO(data))
            member = tarfile.TarInfo("qemu/roms/edk2/EmulatorPkg/Unix/Host/X11IncludeHack")
            member.type = tarfile.SYMTYPE
            member.linkname = "/opt/X11/include"
            output.addfile(member)
        return archive

    def test_qemu_archive_rejects_external_links_in_build_sources(self):
        archive = self.qemu_archive()
        with tarfile.open(archive, "a") as output:
            member = tarfile.TarInfo("qemu/include/escape")
            member.type = tarfile.SYMTYPE
            member.linkname = "/etc"
            output.addfile(member)
        result = self.files("unpack-qemu", archive, self.root / "qemu-source")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "qemu-source/qemu/include/escape").exists())

    def source_setup(self):
        return self.run_script("setup.sh", "--repo-dir", self.root / "new-repo",
                               "--local-copy", self.repo, "--qemu-from", "source",
                               "--qemu-prefix", self.root / "private-qemu",
                               "--qemu-archive", self.qemu_archive(), "--yes")

    def test_source_setup_stages_and_publishes_private_qemu(self):
        result = self.source_setup()
        self.require_success(result)
        self.assertTrue((self.root / "private-qemu/install-manifest.txt").is_file())
        self.assertIn("boot_verified: 1", result.stdout)

    def test_source_setup_rejects_unapproved_installation_paths(self):
        self.env["SETUP_TEST_INSTALL_ESCAPE"] = "1"
        result = self.source_setup()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "private-qemu").exists())
        self.assertFalse((self.root / "cargo-calls").exists())

    def test_invalid_options_do_not_create_source(self):
        for args in (("--qemu-version", "../../escape"), ("--qemu-from", "invalid"),
                     ("--repo-dir",), ("--qemu-version", "10.2.4")):
            with self.subTest(args=args):
                result = self.run_script("setup.sh", *args)
                self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "cargo-calls").exists())


if __name__ == "__main__":
    if sys.argv[1:] == ["--syntax-only"]:
        for source in PACKAGE.rglob("*.py"):
            ast.parse(source.read_text(), filename=str(source))
        print("Python syntax checks passed")
    else:
        unittest.main(verbosity=2)
