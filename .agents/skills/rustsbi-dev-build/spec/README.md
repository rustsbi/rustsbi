<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Local Linux build design

## Goals

Build the selected RustSBI checkout on a prepared Linux host and reproduce
the relevant Library and Prototyper CI commands locally.
Keep the caller's edits, record commands and failures, and verify promised
artifacts before reporting success.
System provisioning, Git mutations, stable/MSRV policy matrices and downstream
operating-system integrations are outside this first version.

## Overview

The skill selects a suite from the user's request.
A standalone Rust CLI parses one raw argument string, resolves the checkout,
constructs a deterministic command plan and writes a result directory.
The same plan is used by `plan` and `run`.
Shell code only bootstraps the CLI and dispatches development tasks.

## Key design

- Require an explicit mode, suite and new output directory.
  Parse quoting in the CLI; do not evaluate the input as a shell program.
- Use the repository's pinned toolchain and Cargo aliases.
  Disable rustup automatic installation; missing native tools or toolchains
  are reported for separate environment preparation.
- Keep build output in the checkout's `target/` because the existing dynamic
  and jump boot script uses those paths.
  The CLI bootstrap has its own ignored cache below the skill's `target/`.
  Planning may compile the CLI and writes the requested plan, but executes
  none of the planned build commands.
- Run commands serially and stop at the first failure.
  Hold a checkout lock to prevent overlapping runs from changing xtask inputs.
  Never infer success from an old artifact or a matching log line alone.
  Record remaining steps as not run, and retain the failed command's log.
- Reuse the existing QEMU verification script and xtask console checks.
  Keep packed-image generation distinct from verified firmware boots.
- Follow the current core CI recipes, with their source job recorded per step.
  The mapping is maintained in [../references/ci-map.md](../references/ci-map.md);
  this is not a general GitHub Actions interpreter.

## Execution

1. Parse the raw input, validate the Linux host and resolve paths.
2. Read workspace manifests and Git metadata, then construct the selected plan.
3. Reserve a new output directory and persist its plan and status.
4. For `run`, check required commands and the selected toolchain without installs.
5. Execute each step in the selected checkout, capturing stdout and stderr.
6. Check required artifacts and persist each step's exit status.
7. Write the final JSON/Markdown report and append usage metadata.

## Components

`scripts/rustsbi-build-cli` contains the standalone Rust CLI and its integration tests.
`scripts/run.sh` forwards one raw input string to that CLI.
`references/ci-map.md` defines coverage and its CI sources.
`agent_profiles/` and `benchmark/` drive pi acceptance scenarios without
embedding credentials or a local service URL.
`usage/` describes the append-only local record; generated logs are ignored.
There are no review personas or model-generated findings in this workflow.

## Verification and implementation plan

Implement argument/path validation and the command plan first, followed by
execution, reports and usage records, then the skill entrypoint and pi driver.
Exercise output collisions, quoted paths, failed commands, missing artifacts
and missing prerequisites in isolated integration tests.
Run the real core suites through pi in a separate WSL checkout, debug observed
failures, and retain the result logs outside the deliverable package.
Stop after validation with the changes uncommitted.
