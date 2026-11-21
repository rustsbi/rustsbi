#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0
set -euo pipefail
skill_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cli=${RQS_CLI:-$skill_dir/scripts/rustsbi-setup-cli/target/release/rustsbi-setup-cli}
[[ -x $cli ]] || { printf 'Build rustsbi-setup-cli first, or set RQS_CLI to a prebuilt binary\n' >&2; exit 3; }
exec "$cli" preflight "$@"
