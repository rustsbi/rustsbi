<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Usage records

The runner appends one JSON object per completed plan or run to `USAGE.log`.
Records include timestamp, trigger, mode/suite, output path, Git revision and
branch, optional agent profile/model, result and failure reason.
Prompt text, source content and credentials are not recorded.
Detailed step states and logs live in the explicitly selected output directory.
Generated usage logs are ignored; set `RDB_USAGE_DIR` for a shared or external
record location without modifying the package.
