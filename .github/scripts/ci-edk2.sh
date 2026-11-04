#!/usr/bin/env bash

CI_EDK2_SCRIPT=$(realpath "${BASH_SOURCE[0]}")

# The script hash covers the pinned commit and the RISC-V QEMU build recipe.
ci_edk2_build_id() (
  set -eo pipefail
  {
    sha256sum < "$CI_EDK2_SCRIPT"
    gcc --version
    riscv64-linux-gnu-gcc --version
    riscv64-linux-gnu-ld --version
    python3 --version
    nasm -v
    iasl -v
  } | sha256sum | cut -d ' ' -f 1
)

ci_edk2_build() (
  set -euo pipefail
  local cache=$1 timeout=$2 workspace
  local revision=6951dfe7d59d144a3a980bd7eda699db2d8554ac
  workspace=$(mktemp -d)
  trap 'rm -rf "$workspace"' EXIT
  cd "$workspace"
  git init --quiet edk2
  git -C edk2 remote add origin https://github.com/tianocore/edk2.git
  timeout "$timeout" git -C edk2 fetch --quiet --depth=1 origin "$revision"
  git -C edk2 checkout --quiet --detach "$revision"
  timeout "$timeout" git -C edk2 submodule update --init --depth=1

  export WORKSPACE="$PWD" PACKAGES_PATH="$PWD/edk2" EDK_TOOLS_PATH="$PWD/edk2/BaseTools"
  export GCC5_RISCV64_PREFIX=riscv64-linux-gnu-
  # edksetup probes optional environment variables.
  set +u
  # shellcheck disable=SC1091
  source "$PACKAGES_PATH/edksetup.sh" --reconfig
  # Newer host GCC versions promote legacy BaseTools warnings to errors.
  make -C "$EDK_TOOLS_PATH" -j"$(nproc)" \
    EXTRA_OPTFLAGS="-Wno-error=discarded-qualifiers"
  # shellcheck disable=SC1091
  source "$PACKAGES_PATH/edksetup.sh" BaseTools
  set -u
  build -a RISCV64 -b RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5

  cd "$WORKSPACE/Build/RiscVVirtQemu/RELEASE_GCC5/FV"
  cp RISCV_VIRT_CODE.fd RISCV_VIRT_VARS.fd "$cache/"
  cd "$cache"
  truncate -s 32M RISCV_VIRT_CODE.fd RISCV_VIRT_VARS.fd
  sha256sum RISCV_VIRT_CODE.fd RISCV_VIRT_VARS.fd > "$workspace/SHA256SUMS"
  mv "$workspace/SHA256SUMS" .
)

# Give each guest its own flash files so its writable variables stay out of the cache.
ci_edk2_prepare() (
  set -euo pipefail
  local guest_dir build_id cache
  guest_dir=$(realpath "$1")
  mkdir -p "$guest_dir"
  build_id=${CI_EDK2_BUILD_ID:-$(ci_edk2_build_id)}
  cache=".cache/ci-edk2/$build_id"
  mkdir -p "$cache"
  cd "$cache"
  if [[ -s SHA256SUMS ]] && sha256sum --check --status SHA256SUMS; then
    echo "Using cached EDK II firmware"
  else
    ci_edk2_build "$PWD" "${2:-15m}"
  fi
  cp RISCV_VIRT_CODE.fd RISCV_VIRT_VARS.fd "$guest_dir/"
)
