#!/usr/bin/env bash
# Remove cache entries that pull requests and short-lived branches wrote before
# cache writes became main-only. Caches are disposable, and these private
# copies consume the same repository quota without being visible to other PRs.
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

default_ref="refs/heads/main"
dry_run=${DRY_RUN:-false}
deleted=0
deleted_bytes=0
cache_list=$(mktemp "${TMPDIR:-/tmp}/rustsbi-cache-list.XXXXXX")
trap 'rm -f -- "$cache_list"' EXIT

gh cache list \
  --repo "$GITHUB_REPOSITORY" \
  --limit 10000 \
  --json id,ref,key,sizeInBytes \
  --jq ".[] | select(.ref != \"$default_ref\") | [.id, .ref, .key, .sizeInBytes] | @tsv" \
  >"$cache_list"

while IFS=$'\t' read -r id ref key bytes; do
  if [[ "$dry_run" == "true" ]]; then
    printf 'Would delete cache %s on %s (%s bytes)\n' "$key" "$ref" "$bytes"
  else
    gh cache delete "$id" --repo "$GITHUB_REPOSITORY"
    printf 'Deleted cache %s on %s (%s bytes)\n' "$key" "$ref" "$bytes"
  fi
  deleted=$((deleted + 1))
  deleted_bytes=$((deleted_bytes + bytes))
done <"$cache_list"

mib=$((deleted_bytes / 1024 / 1024))
action=deleted
if [[ "$dry_run" == "true" ]]; then
  action=would-delete
fi
printf '%s %d non-main cache entries (%d MiB)\n' "$action" "$deleted" "$mib"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "### Cache maintenance"
    echo
    echo "${action} **${deleted}** entries outside \`${default_ref}\` (${mib} MiB)."
  } >>"$GITHUB_STEP_SUMMARY"
fi
