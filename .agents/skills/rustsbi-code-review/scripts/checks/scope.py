"""scope.py - resolve the target and determine what to check.

Parses the same raw argument string as resolve_target.py, extracts
changed paths, maps them to cargo packages, and computes a fingerprint
for caching.
"""

import hashlib
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import ScopeInfo, run_cmd, die

NL = chr(10)
PR_REF = re.compile(r"^(?:pr/|pull/)([0-9]+)")


def tokenize(raw):
    toks, cur, quoted = [], [], False
    for ch in raw:
        if ch == '"':
            quoted = not quoted
        elif ch.isspace() and not quoted:
            if cur:
                toks.append("".join(cur))
                cur = []
        else:
            cur.append(ch)
    if cur:
        toks.append("".join(cur))
    return toks


def git_env():
    """Return env vars to make git work in a linked worktree on WSL.

    In a worktree, .git is a file pointing to the main repo's gitdir.
    On Windows this resolves natively; on WSL the E:/ path does not,
    so we set GIT_DIR/GIT_WORK_TREE explicitly.
    """
    env = {}
    dotgit = os.path.join(os.getcwd(), ".git")
    if os.path.isfile(dotgit):
        with open(dotgit, encoding="utf-8", errors="replace") as f:
            line = f.read().strip()
        if line.startswith("gitdir:"):
            gitdir = line[len("gitdir:"):].strip()
            # Normalize Windows path to WSL mount if needed
            if re.match(r"^[A-Za-z]:[/\\]", gitdir):
                drive = gitdir[0].lower()
                gitdir = "/mnt/{}/{}".format(drive, gitdir[3:].replace("\\", "/"))
            env["GIT_DIR"] = gitdir
            env["GIT_WORK_TREE"] = os.getcwd()
    return env


def git(args, **kwargs):
    """Run a git command with worktree-aware environment."""
    env = git_env()
    env_extra = kwargs.pop("env_extra", None)
    if env_extra:
        env.update(env_extra)
    full_args = ["git"] + args
    # Add -c core.autocrlf=true to handle CRLF on Windows/WSL
    if args and args[0] in ("diff", "status"):
        full_args = ["git", "-c", "core.autocrlf=true"] + args
    return run_cmd(full_args, env_extra=env, **kwargs)


def cargo_metadata():
    """Return list of (package_name, relative_pkg_dir) from cargo metadata.

    Paths are normalized to be relative to the current working directory
    (the repo root) so they match git's relative paths.
    """
    rc, out, err, _ = run_cmd(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        timeout=30)
    if rc != 0:
        die("error: cargo metadata failed: " + first_line(err))
    cwd = os.getcwd().replace("\\", "/")
    pkgs = []
    for p in json.loads(out).get("packages", []):
        manifest = p.get("manifest_path", "").replace("\\", "/")
        pkg_dir = os.path.dirname(manifest)
        # Make relative to cwd
        if pkg_dir.startswith(cwd + "/"):
            pkg_dir = pkg_dir[len(cwd) + 1:]
        elif pkg_dir == cwd:
            pkg_dir = ""
        pkgs.append((p["name"], pkg_dir))
    return pkgs


def map_paths_to_packages(changed_paths, packages):
    """Map changed file paths to cargo package names.

    Both changed_paths and package dirs are relative to the repo root.
    Returns (affected_package_names, all_packages_flag).
    """
    # Workspace-level files that force checking everything
    WORKSPACE_FILES = {
        "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
        "rustfmt.toml", ".cargo/config.toml",
    }
    for p in changed_paths:
        basename = os.path.basename(p)
        if basename in WORKSPACE_FILES or p in WORKSPACE_FILES:
            return [name for name, _ in packages], True

    affected = set()
    for path in changed_paths:
        path_norm = path.replace("\\", "/")
        # Also try making it relative if absolute
        cwd = os.getcwd().replace("\\", "/")
        if path_norm.startswith(cwd + "/"):
            path_norm = path_norm[len(cwd) + 1:]
        for name, pkg_dir in packages:
            if not pkg_dir:
                continue  # skip root-level
            if path_norm.startswith(pkg_dir + "/") or path_norm == pkg_dir:
                affected.add(name)
                break
    return sorted(affected), False


def compute_fingerprint(scope):
    """Compute a stable fingerprint for caching.

    Includes: head sha, dirty state, affected packages, toolchain hash.
    """
    h = hashlib.sha256()
    h.update(scope.mode.encode())
    h.update(scope.head.encode())
    h.update(str(scope.dirty).encode())
    h.update(",".join(sorted(scope.affected_packages)).encode())
    # Include toolchain info
    rc, out, _, _ = run_cmd(["rustc", "--version"], timeout=10)
    h.update(out.strip().encode())
    rc, out, _, _ = run_cmd(["cargo", "--version"], timeout=10)
    h.update(out.strip().encode())
    return h.hexdigest()[:16]


def run_scope(raw_args):
    """Parse raw args and return a ScopeInfo.

    For pr/N mode, returns ScopeInfo with skipped_reason set.
    """
    toks = tokenize(raw_args)
    if len(toks) < 3:
        die("usage: diff <base|pr/N> <output> | files <path[:lines]> ... <output>")

    mode, output = toks[0], toks[-1]
    targets = toks[1:-1]

    if PR_REF.match(targets[0]) if mode == "diff" and targets else False:
        return ScopeInfo(mode="pr", skipped_reason="backtest: pr/N does not check out")

    if mode == "diff":
        base = targets[0]
        rc, head, err, _ = git(["rev-parse", "--short", "HEAD"])
        if rc != 0:
            die("error: git rev-parse HEAD failed: " + err.strip())
        head = head.strip()

        rc, branch, _, _ = git(["rev-parse", "--abbrev-ref", "HEAD"])
        branch = branch.strip()

        # Changed paths in the range
        rc, mb, _, _ = git(["merge-base", base, "HEAD"])
        if rc != 0:
            die("error: git merge-base {} HEAD failed".format(base))
        mb = mb.strip()

        rc, out, err, _ = git(["diff", "--name-only", mb + "..HEAD"])
        changed = [l.strip() for l in out.splitlines() if l.strip()]

        # Check dirty
        rc, out, _, _ = git(["status", "--porcelain"])
        dirty = bool(out.strip())

        # Map to packages
        packages = cargo_metadata()
        affected, all_pkgs = map_paths_to_packages(changed, packages)

        scope = ScopeInfo(
            mode="diff", base=base, head=head, branch=branch,
            changed_paths=changed, affected_packages=affected,
            dirty=dirty, all_packages=all_pkgs)
        scope.fingerprint = compute_fingerprint(scope)
        return scope

    elif mode == "files":
        paths = []
        for tok in targets:
            path = tok.split(":")[0] if ":" in tok else tok
            if os.path.isfile(path):
                paths.append(path)
        packages = cargo_metadata()
        affected, all_pkgs = map_paths_to_packages(paths, packages)

        rc, head, _, _ = git(["rev-parse", "--short", "HEAD"])
        head = head.strip()

        scope = ScopeInfo(
            mode="files", head=head,
            changed_paths=paths, affected_packages=affected,
            all_packages=all_pkgs)
        scope.fingerprint = compute_fingerprint(scope)
        return scope

    else:
        die("error: unknown mode: {}".format(mode))
