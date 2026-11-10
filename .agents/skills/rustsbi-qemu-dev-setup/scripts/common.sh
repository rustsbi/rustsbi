#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0

QEMU_MIN=9.1
QEMU_DEFAULT=10.2.1
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Usage: preflight.sh|setup.sh [options]
  --repo-dir DIR          existing RustSBI workspace or absent destination
  --local-copy PATH       local RustSBI directory or tar archive
  --qemu-from MODE        auto (default), distro (reuse only), or source
  --qemu-version VER      source version (default 10.2.1)
  --qemu-prefix DIR       new private installation prefix
  --qemu-archive PATH     local source archive instead of a QEMU download
  --no-test              build without boot verification
  --plan                 print the write inventory without changing files
  --yes                  acknowledge setup writes after reviewing the inventory
  --help                 show this help
EOF
}

parse_options() {
    REPO_DIR=$HOME/rustsbi
    LOCAL_COPY=
    QEMU_FROM=auto
    QEMU_WANT=$QEMU_DEFAULT
    QEMU_PREFIX=
    QEMU_ARCHIVE=
    RUN_TEST=1
    ASSUME_YES=0
    PLAN_ONLY=0
    while (( $# )); do
        case "$1" in
            --repo-dir|--local-copy|--qemu-from|--qemu-version|--qemu-prefix|--qemu-archive)
                [[ $# -ge 2 && -n $2 && $2 != --* ]] || die "missing value for $1"
                case "$1" in
                    --repo-dir) REPO_DIR=$2 ;;
                    --local-copy) LOCAL_COPY=$2 ;;
                    --qemu-from) QEMU_FROM=$2 ;;
                    --qemu-version) QEMU_WANT=$2 ;;
                    --qemu-prefix) QEMU_PREFIX=$2 ;;
                    --qemu-archive) QEMU_ARCHIVE=$2 ;;
                esac
                shift 2 ;;
            --no-test) RUN_TEST=0; shift ;;
            --yes) ASSUME_YES=1; shift ;;
            --plan) PLAN_ONLY=1; shift ;;
            --help) usage; exit 0 ;;
            *) die "unknown option: $1" ;;
        esac
    done
    case "$QEMU_FROM" in auto|distro|source) ;; *) die "invalid QEMU mode: $QEMU_FROM" ;; esac
    [[ $QEMU_WANT =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "invalid QEMU version: $QEMU_WANT"
    version_usable "$QEMU_WANT" || die "unsupported QEMU source version: $QEMU_WANT"
    [[ $(uname -s) == Linux ]] || die "Linux is required"
    if ! command -v python3 >/dev/null; then
        printf 'BLOCK host:python3 missing; see README host prerequisites\n'
        exit 3
    fi
    REPO_DIR=$(absolute "$REPO_DIR")
    [[ -z $LOCAL_COPY ]] || LOCAL_COPY=$(absolute "$LOCAL_COPY")
    [[ -z $QEMU_ARCHIVE ]] || QEMU_ARCHIVE=$(absolute "$QEMU_ARCHIVE")
    QEMU_PREFIX=$(absolute "${QEMU_PREFIX:-$HOME/.local/share/rustsbi/qemu/$QEMU_WANT}")
    TOOLS_ROOT=$(absolute "$HOME/.local/share/rustsbi/cargo-tools")
    CARGO_DIR=$(absolute "${CARGO_HOME:-$HOME/.cargo}")
    RUSTUP_DIR=$(absolute "${RUSTUP_HOME:-$HOME/.rustup}")
    local source_args=("$REPO_DIR")
    [[ -z $LOCAL_COPY ]] || source_args+=("$LOCAL_COPY")
    python3 -B "$HERE/files.py" check-source "${source_args[@]}" >/dev/null
    if [[ -n $QEMU_ARCHIVE ]]; then
        [[ -f $QEMU_ARCHIVE ]] || die "QEMU archive does not exist: $QEMU_ARCHIVE"
    fi
}

absolute() { python3 -B "$HERE/files.py" absolute "$1"; }

version_usable() {
    [[ $1 =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ && $1 != 10.2.4 ]] || return 1
    [[ $(printf '%s\n%s\n' "$QEMU_MIN" "$1" | sort -V | tail -n 1) == "$1" ]]
}

qemu_version() {
    local output version
    output=$("$1" --version) || return 1
    read -r _ _ _ version _ <<< "${output%%$'\n'*}"
    [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || return 1
    printf '%s\n' "$version"
}

select_qemu() {
    QEMU_BIN=
    BUILD_QEMU=1
    local candidate version
    if [[ $QEMU_FROM != source ]]; then
        candidate=$(command -v qemu-system-riscv64 || true)
        if [[ -n $candidate ]] && version=$(qemu_version "$candidate") && version_usable "$version"; then
            QEMU_BIN=$(absolute "$candidate")
            BUILD_QEMU=0
            return
        fi
        [[ $QEMU_FROM != distro ]] || die "no usable QEMU on PATH (requires >= $QEMU_MIN; excludes 10.2.4)"
    fi
    if [[ -e $QEMU_PREFIX || -L $QEMU_PREFIX ]]; then
        [[ ! -L $QEMU_PREFIX && -f $QEMU_PREFIX/install-manifest.txt ]] ||
            die "refusing existing unmanaged QEMU prefix: $QEMU_PREFIX"
        candidate=$QEMU_PREFIX/bin/qemu-system-riscv64
        if [[ -x $candidate ]] && version=$(qemu_version "$candidate") && [[ $version == "$QEMU_WANT" ]]; then
            QEMU_BIN=$candidate
            BUILD_QEMU=0
        else
            die "QEMU prefix cannot be reused; choose a new --qemu-prefix"
        fi
    fi
}

host_prerequisites() {
    local tool missing=0
    local tools=(git curl cc python3)
    if (( BUILD_QEMU )); then tools+=(c++ make ninja meson pkg-config); fi
    for tool in "${tools[@]}"; do
        if command -v "$tool" >/dev/null; then
            printf 'PASS  host:%s present\n' "$tool"
        else
            printf 'BLOCK host:%s missing; prepare native dependencies listed in README\n' "$tool"
            missing=1
        fi
    done
    if ! python3 -B -c 'import tarfile; assert hasattr(tarfile, "data_filter")' >/dev/null 2>&1; then
        printf 'BLOCK host:tarfile Python tarfile.data_filter is required for safe extraction\n'
        missing=1
    fi
    if (( BUILD_QEMU )); then
        if ! command -v pkg-config >/dev/null || ! pkg-config --exists glib-2.0 gthread-2.0; then
            printf 'BLOCK host:glib GLib development files are required for a QEMU source build\n'
            missing=1
        fi
        if ! python3 -B -c 'import ensurepip, venv' >/dev/null 2>&1; then
            printf 'BLOCK host:venv Python venv and ensurepip are required\n'
            missing=1
        fi
    fi
    (( missing == 0 ))
}

write_inventory() {
    printf 'Repository: %s\n' "$REPO_DIR"
    if [[ -n $LOCAL_COPY ]]; then
        printf 'Source: copy %s into the new repository\n' "$LOCAL_COPY"
    elif [[ -d $REPO_DIR ]]; then
        printf 'Source: reuse the selected workspace\n'
    else
        printf 'Source: clone https://github.com/rustsbi/rustsbi.git\n'
    fi
    if (( BUILD_QEMU )); then
        printf 'QEMU: build %s into new private prefix %s\n' "$QEMU_WANT" "$QEMU_PREFIX"
        printf 'QEMU source: %s\n' "${QEMU_ARCHIVE:-https://download.qemu.org/qemu-$QEMU_WANT.tar.xz}"
        printf 'QEMU payload: emulator, data, headers/libraries and install-manifest.txt inside that prefix\n'
        printf 'Temporary files: source/build under %s and staged installation beside the prefix\n' "${TMPDIR:-/tmp}"
    else
        printf 'QEMU: reuse %s\n' "$QEMU_BIN"
    fi
    printf 'Rust homes: %s and %s (missing toolchain/components and registry caches)\n' "$RUSTUP_DIR" "$CARGO_DIR"
    printf 'cargo-binutils: install missing tools into %s\n' "$TOOLS_ROOT"
    printf 'Build outputs: %s/target; boot verification requested: %s\n' "$REPO_DIR" "$RUN_TEST"
    printf 'System package managers: not invoked; shell startup files: not modified\n'
}
