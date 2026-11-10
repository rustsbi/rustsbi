#!/usr/bin/env python3
# SPDX-License-Identifier: MulanPSL-2.0

"""Validate, stage and publish setup files without replacing existing paths."""

import argparse
import ctypes
import errno
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile


def absolute(value):
    path = Path(os.path.abspath(os.path.expanduser(value)))
    return path.parent.resolve() / path.name


def workspace(path):
    return all((path / name).is_file() for name in (
        "Cargo.toml", "rust-toolchain.toml", "firmware/prototyper/Cargo.toml",
        "library/rustsbi/Cargo.toml",
    ))


def check_source(target, source=None):
    if target.is_symlink():
        raise ValueError(f"refusing symlink destination: {target}")
    if target.exists():
        if source is None and target.is_dir() and workspace(target):
            return "reuse"
        raise ValueError(f"destination already exists: {target}; use an absent path")
    if source is not None:
        if not source.exists():
            raise ValueError(f"local source does not exist: {source}")
        if source.is_dir() and (target == source or source in target.parents):
            raise ValueError("destination must not be inside the local source")
        if not source.is_dir() and not tarfile.is_tarfile(source):
            raise ValueError("local source must be a directory or tar archive")
    return "new"


def publish(source, destination):
    # Both paths are on the same filesystem. Fail closed if no-replace rename
    # is unavailable; a check followed by ordinary rename would race a writer.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = getattr(libc, "renameat2", None)
    if rename is None:
        raise ValueError("atomic no-replace publication requires Linux glibc")
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int,
                       ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(destination), 1):
        code = ctypes.get_errno()
        if code == errno.EEXIST:
            raise ValueError(f"destination appeared during setup: {destination}")
        raise OSError(code, os.strerror(code), str(destination))


def prepare_source(target, source):
    if check_source(target, source) == "reuse":
        print(target)
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".rustsbi-source-", dir=target.parent) as temp:
        staged = Path(temp) / "source"
        if source is None:
            subprocess.run(["git", "clone", "https://github.com/rustsbi/rustsbi.git",
                            str(staged)], check=True)
        elif source.is_dir():
            shutil.copytree(source, staged, symlinks=True)
        else:
            if not hasattr(tarfile, "data_filter"):
                raise ValueError("safe tar extraction requires Python tarfile.data_filter")
            staged.mkdir()
            with tarfile.open(source) as archive:
                archive.extractall(staged, filter="data")
            if not workspace(staged):
                entries = list(staged.iterdir())
                if len(entries) == 1 and entries[0].is_dir():
                    staged = entries[0]
        if not workspace(staged):
            raise ValueError("local source does not contain a RustSBI workspace")
        publish(staged, target)
    print(target)


def qemu_source_filter(member, destination):
    # EDK II's unused host sources contain an absolute X11 header symlink.
    # This setup does not build ROM firmware; retain data filtering elsewhere.
    parts = PurePosixPath(member.name).parts
    if parts[:2] == ("roms", "edk2") or parts[1:3] == ("roms", "edk2"):
        return None
    return tarfile.data_filter(member, destination)


def unpack_qemu(archive_path, target):
    if not hasattr(tarfile, "data_filter"):
        raise ValueError("safe tar extraction requires Python tarfile.data_filter")
    target.mkdir()
    with tarfile.open(archive_path) as archive:
        archive.extractall(target, filter=qemu_source_filter)
    entries = list(target.iterdir())
    source = entries[0] if len(entries) == 1 and entries[0].is_dir() else target
    if not (source / "configure").is_file() or not (source / "meson.build").is_file():
        raise ValueError("archive does not contain a QEMU source tree")
    print(source)


def install_prefix(stage, prefix):
    if prefix.exists() or prefix.is_symlink():
        raise ValueError(f"installation prefix already exists: {prefix}")
    payload = stage / str(prefix).lstrip("/")
    if payload.is_symlink():
        raise ValueError("staged prefix must be a directory, not a symlink")
    if not (payload / "bin/qemu-system-riscv64").is_file():
        raise ValueError("staged QEMU binary is missing")
    entries = []
    for path in sorted(stage.rglob("*")):
        if path == payload:
            continue
        if payload in path.parents:
            entry = path.relative_to(payload).as_posix()
            if path.is_dir() and not path.is_symlink():
                entry += "/"
            entries.append(entry)
        elif path.is_symlink() or not path.is_dir() or path not in payload.parents:
            raise ValueError(f"installation escapes the approved prefix: {path}")
    manifest = payload / "install-manifest.txt"
    if manifest.exists() or manifest.is_symlink():
        raise ValueError("upstream payload already contains install-manifest.txt")
    manifest.write_text("\n".join(entries + ["install-manifest.txt"]) + "\n")
    print(f"QEMU installation manifest for {prefix}:", flush=True)
    for entry in entries + ["install-manifest.txt"]:
        print(f"  {prefix}/{entry}", flush=True)
    prefix.parent.mkdir(parents=True, exist_ok=True)
    publish(payload, prefix)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("absolute", "check-source", "prepare-source"):
        command = sub.add_parser(name)
        command.add_argument("target")
        if name != "absolute":
            command.add_argument("source", nargs="?")
    command = sub.add_parser("install-prefix")
    command.add_argument("stage")
    command.add_argument("prefix")
    command = sub.add_parser("unpack-qemu")
    command.add_argument("archive")
    command.add_argument("target")
    args = parser.parse_args()
    if args.command == "absolute":
        print(absolute(args.target))
    elif args.command == "install-prefix":
        install_prefix(absolute(args.stage), absolute(args.prefix))
    elif args.command == "unpack-qemu":
        unpack_qemu(absolute(args.archive), absolute(args.target))
    else:
        target = absolute(args.target)
        source = absolute(args.source) if args.source else None
        if args.command == "check-source":
            print(check_source(target, source))
        else:
            prepare_source(target, source)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"setup files: {error}") from error
