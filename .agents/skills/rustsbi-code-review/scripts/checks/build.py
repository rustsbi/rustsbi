"""build.py - cargo check/build matrix for affected packages.

Compile failure blocks the LLM review (gate="compile").
Missing tools or targets are reported as skip/error, not fail.
"""

import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import CheckResult, run_cmd, first_line, truncate

NL = chr(10)

RV64 = "riscv64imac-unknown-none-elf"
RV64GC = "riscv64gc-unknown-none-elf"
RV32 = "riscv32imac-unknown-none-elf"

# Packages that don't support RV32 (from Library.yml)
NO_RV32 = {"pmpm", "smm"}

# Feature matrix for rustsbi (from Library.yml)
RUSTSBI_FEATURES = [[], ["machine"], ["forward"], ["machine,forward"]]

# Firmware packages that need cargo-binutils for prototyper build
FIRMWARE_PACKAGES = {"rustsbi-prototyper", "rustsbi-test-kernel", "rustsbi-bench-kernel"}


def has_tool(name):
    """Check if a tool is on PATH or in ~/.cargo/bin."""
    if shutil.which(name):
        return True
    cargo_bin = os.path.expanduser("~/.cargo/bin")
    if os.name == "nt":
        path = os.path.join(cargo_bin, name + ".exe")
    else:
        path = os.path.join(cargo_bin, name)
    return os.path.isfile(path)


def tool_env():
    """Return env additions to include ~/.cargo/bin in PATH."""
    cargo_bin = os.path.expanduser("~/.cargo/bin")
    if os.path.isdir(cargo_bin):
        return {"PATH": cargo_bin + os.pathsep + os.environ.get("PATH", "")}
    return {}


def make_check_id(pkg, target, features=""):
    parts = ["check", pkg]
    if target:
        parts.append(target.split("-")[0])  # e.g. "riscv64imac..." -> "riscv64imac"
    if features:
        parts.append(features.replace(",", "+"))
    return "-".join(parts)


def check_package(pkg, target=None, features=None, gate="compile"):
    """Run cargo check for one package/target/feature combo."""
    args = ["cargo", "check", "--locked", "-p", pkg]
    if target:
        args += ["--target", target]
    if features:
        args += ["--features", features]
    cmd_str = " ".join(args)
    rc, out, err, ms = run_cmd(args, timeout=600)
    if rc == 0:
        return CheckResult(make_check_id(pkg, target, features), "build",
                           "pass", gate, ms, cmd_str, "ok")
    # Distinguish compile error from environment error
    is_env = any(s in err.lower() for s in [
        "could not find cargo", "toolchain", "target not installed",
        "no such file", "permission denied"])
    status = "error" if is_env else "fail"
    return CheckResult(make_check_id(pkg, target, features), "build",
                       status, gate, ms, cmd_str,
                       first_line(err or out), truncate(err or out))


def build_package(pkg, target=None, features=None, gate="compile"):
    """Run cargo build for one package/target/feature combo."""
    args = ["cargo", "build", "--locked", "-p", pkg]
    if target:
        args += ["--target", target]
    if features:
        args += ["--features", features]
    cmd_str = " ".join(args)
    rc, out, err, ms = run_cmd(args, timeout=600)
    if rc == 0:
        return CheckResult("build-" + make_check_id(pkg, target, features).replace("check-", ""),
                           "build", "pass", gate, ms, cmd_str, "ok")
    is_env = any(s in err.lower() for s in [
        "could not find cargo", "toolchain", "target not installed",
        "no such file", "permission denied", "rust-objcopy"])
    status = "error" if is_env else "fail"
    return CheckResult("build-" + make_check_id(pkg, target, features).replace("check-", ""),
                       "build", status, gate, ms, cmd_str,
                       first_line(err or out), truncate(err or out))


# Packages that only build on the host (not for RISC-V targets)
HOST_ONLY = {"xtask"}


def run_build(scope):
    """Determine and run the build matrix. Returns list of CheckResult."""
    if scope.skipped_reason:
        return [CheckResult("build", "build", "skip", "compile", 0,
                            "", scope.skipped_reason)]
    if not scope.affected_packages:
        # Docs-only or non-code change
        return [CheckResult("build", "build", "skip", "compile", 0,
                            "", "no affected packages (docs/metadata only)")]

    results = []

    # Separate firmware from library packages
    lib_pkgs = [p for p in scope.affected_packages
                if p not in FIRMWARE_PACKAGES and p not in HOST_ONLY]
    fw_pkgs = [p for p in scope.affected_packages if p in FIRMWARE_PACKAGES]
    host_pkgs = [p for p in scope.affected_packages if p in HOST_ONLY]

    # Host-only packages (xtask): just check on host
    for pkg in host_pkgs:
        results.append(check_package(pkg, target=None, gate="compile"))

    # Library packages: cargo check on rv64 (and rv32 if supported)
    for pkg in lib_pkgs:
        # Host check (fast)
        results.append(check_package(pkg, target=None, gate="compile"))
        # rv64 check
        results.append(check_package(pkg, target=RV64, gate="compile"))
        # rv32 check (skip pmpm/smm)
        if pkg not in NO_RV32:
            results.append(check_package(pkg, target=RV32, gate="compile"))

    # rustsbi: feature matrix builds
    if "rustsbi" in scope.affected_packages:
        for feats in RUSTSBI_FEATURES:
            feat_str = ",".join(feats) if feats else ""
            results.append(build_package("rustsbi", target=RV64, features=feat_str))
            results.append(build_package("rustsbi", target=RV32, features=feat_str))

    # sbi-testing: log feature build
    if "sbi-testing" in scope.affected_packages:
        results.append(build_package("sbi-testing", target=RV64, features="log"))

    # Firmware packages: check if cargo-binutils available
    if fw_pkgs:
        if not has_tool("rust-objcopy"):
            results.append(CheckResult(
                "build-firmware", "build", "skip", "compile", 0,
                "", "cargo-binutils not installed; skipping firmware build"))
        else:
            # Prototyper needs cargo prototyper build first (generates config)
            if "rustsbi-prototyper" in fw_pkgs:
                rc, out, err, ms = run_cmd(
                    ["cargo", "prototyper", "build"], timeout=900,
                    env_extra=tool_env())
                if rc != 0:
                    results.append(CheckResult(
                        "prototyper-build", "build", "fail", "compile", ms,
                        "cargo prototyper build",
                        first_line(err or out), truncate(err or out)))
                else:
                    results.append(CheckResult(
                        "prototyper-build", "build", "pass", "compile", ms,
                        "cargo prototyper build", "ok"))
            for pkg in fw_pkgs:
                target = RV64GC if pkg == "rustsbi-prototyper" else RV64
                results.append(check_package(pkg, target=target, gate="compile"))

    return results
