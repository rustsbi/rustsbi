#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0
set -euo pipefail
skill_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
: "${RDB_PI_MODEL:?set an explicit configured pi model}"
: "${RDB_SMOKE_REPO:?set the isolated checkout path}"
: "${RDB_SMOKE_OUTPUT:?set a new absolute evidence directory}"
[[ $RDB_SMOKE_OUTPUT == /* ]] || { printf 'Use an absolute output path\n' >&2; exit 2; }
mkdir -- "$RDB_SMOKE_OUTPUT"
cd -- "$RDB_SMOKE_REPO"
export RDB_TRIGGER=smoke RDB_AGENT_PROFILE=pi RDB_MODEL="$RDB_PI_MODEL"
export RDB_USAGE_DIR="$RDB_SMOKE_OUTPUT/usage" PI_OFFLINE=1
prompt="$(cat "$skill_dir/benchmark/request.md")
Write the build results to: $RDB_SMOKE_OUTPUT/build"
printf '%s\n' "$RDB_PI_MODEL" > "$RDB_SMOKE_OUTPUT/model.txt"
pi_status=0
pi --print --approve --offline --no-extensions --no-mcp --tools read,bash \
    --mode json --model "$RDB_PI_MODEL" --thinking medium \
    --skill "$skill_dir/SKILL.md" --session-dir "$RDB_SMOKE_OUTPUT/sessions" \
    "$prompt" < /dev/null > "$RDB_SMOKE_OUTPUT/pi.jsonl" 2>&1 || pi_status=$?
bash "$skill_dir/scripts/run.sh" verify-acceptance "$RDB_SMOKE_OUTPUT" "$RDB_SMOKE_REPO" "$pi_status"
