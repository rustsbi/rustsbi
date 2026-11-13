<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Host adapters

Codex and Claude Code use `SKILL.md` in the current session and invoke the same
Rust CLI with one raw argument. There is no nested model pass in a build run.
The `.agents/skills` package is the instruction source; Claude's project entry
is a relative symlink to that package.
On Windows, a checkout with `core.symlinks=false` contains a target-path file
instead of a working link. Use a Linux checkout with symlinks enabled for that
entry, or enable native Windows symlink support before checking it out.

The pi profile is an acceptance driver for an already configured local pi.
The shell adapter passes this fixed profile directly to pi.
`RDB_PI_MODEL` selects an explicit model and pi supplies its existing credentials.
The profile fixes medium thinking, disables extensions/MCP, and enables read/bash.
No service URL or authentication material belongs in this package.
The result directory records the selected model, pi event stream, session files,
build report and `acceptance.json`. A successful pi process is not enough:
the driver requires a passed `all` run in the selected checkout, with every
planned step and all six QEMU boot checks passed.
Use an isolated checkout for these acceptance runs.
