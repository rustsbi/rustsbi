#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0
set -euo pipefail
skill_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
export RDB_SKILL_HOME="$skill_dir"
cli=${RDB_CLI:-$skill_dir/scripts/rustsbi-build-cli/target/release/rustsbi-dev-build}
if [[ ! -x $cli ]]; then
    RUSTUP_AUTO_INSTALL=0 cargo build --release --locked --manifest-path "$skill_dir/scripts/rustsbi-build-cli/Cargo.toml"
fi
exec "$cli" "$@"
