#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
parse_options "$@"
select_qemu
host_prerequisites || die "prepare native prerequisites before setup; see README"
write_inventory
if (( PLAN_ONLY )); then exit 0; fi
(( ASSUME_YES )) || die "review this inventory and obtain approval before passing --yes"

source_args=("$REPO_DIR")
[[ -z $LOCAL_COPY ]] || source_args+=("$LOCAL_COPY")
python3 -B "$HERE/files.py" prepare-source "${source_args[@]}"

work_dirs=()
cleanup() {
    local directory
    for directory in "${work_dirs[@]}"; do rm -rf -- "$directory"; done
}
trap cleanup EXIT

if (( BUILD_QEMU )); then
    work=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-qemu.XXXXXXXX")
    work_dirs+=("$work")
    archive=$QEMU_ARCHIVE
    if [[ -z $archive ]]; then
        archive=$work/qemu.tar.xz
        curl --proto '=https' --tlsv1.2 --retry 2 -fL \
            "https://download.qemu.org/qemu-$QEMU_WANT.tar.xz" -o "$archive"
    fi
    qemu_source=$(python3 -B "$HERE/files.py" unpack-qemu "$archive" "$work/source")
    mkdir -p -- "$work/build" "$(dirname -- "$QEMU_PREFIX")"
    stage=$(mktemp -d "$(dirname -- "$QEMU_PREFIX")/.rustsbi-qemu-stage.XXXXXXXX")
    work_dirs+=("$stage")
    (
        export XDG_CACHE_HOME="$work/cache" PIP_CACHE_DIR="$work/pip-cache"
        export CCACHE_DIR="$work/ccache" SCCACHE_DIR="$work/sccache"
        cd -- "$work/build"
        "$qemu_source/configure" \
            --prefix="$QEMU_PREFIX" \
            --sysconfdir="$QEMU_PREFIX/etc" \
            --localstatedir="$QEMU_PREFIX/var" \
            --target-list=riscv64-softmmu \
            --disable-docs --disable-werror --disable-tools \
            --disable-guest-agent --disable-plugins --disable-install-blobs
        make -j"${RUSTSBI_SETUP_JOBS:-$(nproc)}"
        make DESTDIR="$stage" install
    )
    staged_binary=$stage$QEMU_PREFIX/bin/qemu-system-riscv64
    [[ $(qemu_version "$staged_binary") == "$QEMU_WANT" ]] || die "staged QEMU version mismatch"
    python3 -B "$HERE/files.py" install-prefix "$stage" "$QEMU_PREFIX"
    QEMU_BIN=$QEMU_PREFIX/bin/qemu-system-riscv64
fi

# The PATH change is local to this process; no login/profile files are edited.
export PATH="$(dirname -- "$QEMU_BIN"):$CARGO_DIR/bin:$TOOLS_ROOT/bin:$PATH"
if ! command -v rustup >/dev/null; then
    installer=$(mktemp "${TMPDIR:-/tmp}/rustsbi-rustup.XXXXXXXX")
    work_dirs+=("$installer")
    curl --proto '=https' --tlsv1.2 --retry 2 -fsSL https://sh.rustup.rs -o "$installer"
    sh "$installer" -y --profile minimal --default-toolchain none --no-modify-path
fi
command -v rustup >/dev/null || die "rustup is unavailable after installation"
cd -- "$REPO_DIR"
export CARGO_TARGET_DIR="$REPO_DIR/target"
# Running cargo here deliberately resolves this checkout's rust-toolchain.toml.
cargo --version
if ! command -v rust-objcopy >/dev/null; then
    cargo install --locked --root "$TOOLS_ROOT" cargo-binutils@0.4.0
fi
cargo build
cargo prototyper build
elf=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf
bin=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
[[ -s $elf && -s $bin ]] || die "required firmware ELF or raw binary is missing"
if (( RUN_TEST )); then cargo prototyper test; fi
printf 'ENVIRONMENT UP\nrepository: %s\nfirmware: %s/%s\nraw binary: %s/%s\n' \
    "$REPO_DIR" "$REPO_DIR" "$elf" "$REPO_DIR" "$bin"
printf 'qemu: %s\nboot_verified: %s\n' "$QEMU_BIN" "$RUN_TEST"
