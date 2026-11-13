<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# RustSBI local build

Run RustSBI's core build and verification commands on a prepared Linux host.
The implementation is a standalone Rust CLI with a thin Bash launcher.
It works in dirty checkouts and records their Git revision and status.

## Quick start

From the checkout to build:

```sh
bash "$(pwd)/.agents/skills/rustsbi-dev-build/scripts/run.sh" \
    'run build /tmp/rustsbi-build-1'
```

Inspect the broader suite before running it:

```sh
bash "$(pwd)/.agents/skills/rustsbi-dev-build/scripts/run.sh" \
    'plan all /tmp/rustsbi-plan-1'
bash "$(pwd)/.agents/skills/rustsbi-dev-build/scripts/run.sh" \
    'run all /tmp/rustsbi-all-1'
```

To select another checkout or a private QEMU installation, use inner quotes:

```sh
bash /absolute/skill/scripts/run.sh \
    'run qemu --repo "/work/RustSBI checkout" --qemu-bin "/home/user/.local/share/rustsbi/qemu/10.2.1/bin/qemu-system-riscv64" "/tmp/boot report"'
```

The whole invocation is one shell argument.
The CLI parses quoting without shell expansion or command substitution.
The output directory is mandatory, last, and must not exist.

## Prerequisites and writes

The host needs Bash, Git, a C compiler, Rust through rustup and the checkout's
pinned toolchain. The launcher also needs an installed native Rust toolchain
to compile the helper. Set `RDB_CLI` to use a prebuilt release binary, or
select an installed host toolchain with `RUSTUP_TOOLCHAIN` before bootstrap.
The launcher never installs a toolchain automatically.
Cargo may fetch the helper's locked dependencies on first use.

Suite-specific prerequisites are written to `plan.json` and checked before
build commands run: Rust components/targets, `rust-objcopy`, QEMU >= 9.1 for
`qemu`, and `mkimage`/`dtc` for `pack`.
`--qemu-bin` must point to an executable named `qemu-system-riscv64`; its
directory is prepended to PATH only for this run.

Firmware and library products use the checkout's `target/`, overriding
`CARGO_TARGET_DIR` for consistent paths with the current boot script.
The helper uses its standalone workspace release target directory.
Reports/logs use the selected new output directory; Cargo uses its normal
registry cache. Usage metadata is appended under `<skill>/usage` by default.
Set `RDB_USAGE_DIR` to redirect that record, and `RDB_TRIGGER`,
`RDB_AGENT_PROFILE`, `RDB_MODEL` to label it when running acceptance checks.
The helper does not install packages, alter shell startup files, commit changes
or run automatic format fixes.

## Results

Each output directory contains `plan.json`, `report.json`, `report.md` and
per-command `logs/`. The QEMU script also writes `qemu-logs/` there.
Reports are updated after every step. An interrupted run can retain a
`running` status; it must not be interpreted as successful.

Exit codes are `0` for a written plan or a passed run, `1` for a failed step or
missing/empty artifact, `2` for invalid input/report errors, and `3` for missing
prerequisites or an overlapping run.
Clippy warnings retain CI's behavior: warnings are visible in logs and do not
become errors unless the repository or caller configures that policy.
See [references/ci-map.md](references/ci-map.md) for the exact coverage.

## Package checks

Run `make check test` in this directory for formatting, Clippy and isolated
integration tests in the standalone Rust workspace.
`make smoke` runs the pi acceptance profile; it requires
`RDB_PI_MODEL`, `RDB_SMOKE_REPO`, and a new `RDB_SMOKE_OUTPUT`.
See [agent_profiles/README.md](agent_profiles/README.md) for the profile boundary
and [spec/README.md](spec/README.md) for the design.
