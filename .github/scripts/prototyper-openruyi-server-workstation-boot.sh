#!/usr/bin/env bash
#
# Boot the pinned openRuyi Creek 2026.09 Server and Workstation images
# through RustSBI Prototyper in QEMU riscv64 virt.
#
# Usage: $0 <server|workstation-labwc|workstation-kde> <sbi|u-boot|edk2>
#        $0 --prepare <server|workstation-labwc|workstation-kde>
#        $0 --self-test
#
# Requires the rva23s64 CPU model (QEMU 10.1+, Ubuntu 26.04 ships 10.2),
# `cargo prototyper build` output, aria2, bsdtar, mtools, qemu-img,
# qemu-system-riscv64 and dpkg-deb. OPENRUYI_CACHE_DIR, OPENRUYI_QEMU,
# OPENRUYI_RUSTSBI, QEMU_LOG_DIR and the various *_TIMEOUT_SECS override
# the defaults.

set -euo pipefail

readonly RELEASE="2026.09"
readonly BASE_URL="https://releases.openruyi.cn/creek/${RELEASE}/rva23"

readonly SERVER_NAME="openRuyi-${RELEASE}-Server-cloud.qcow2"
readonly SERVER_SHA256="2045e94d448ec12dbd3d7613c68005e8fdbe0748faf837f6e212af475dc8bc47"
readonly LABWC_NAME="openRuyi-${RELEASE}-Workstation-labwc-dvd.iso"
readonly LABWC_SHA256="35b6cc6882469836578e398f544b913d38788bae206445d033ee4b343130be04"
readonly KDE_NAME="openRuyi-${RELEASE}-Workstation-kde-dvd.iso"
readonly KDE_SHA256="76fbedf02cd4eca60a5e00d9e60244b24c6f84dca0e56faba05421f4cfa20d63"

readonly EDK2_CODE_NAME="RISCV_VIRT_CODE.fd"
readonly EDK2_CODE_SHA256="4dc955ecb8fa727e3989e8f40c1b846bffc38cd2830601a97eaa620807480c01"
readonly EDK2_VARS_NAME="RISCV_VIRT_VARS.fd"
readonly EDK2_VARS_SHA256="ea8094e953b1215444bd001ee1cf22818f1f7f8abcb158e62180cbaa6c1f70af"

readonly UBOOT_DEB="u-boot-qemu_2025.10-0ubuntu0.24.04.4_all.deb"
readonly UBOOT_SHA256="997cd55ca82e64163595249d1b8117732e6ad4c22a67634e605286cc61508c27"
readonly UBOOT_URL="https://snapshot.ubuntu.com/ubuntu/20261023T000000Z/pool/main/u/u-boot/${UBOOT_DEB}"
readonly UBOOT_BANNER="U-Boot 2025.10-0ubuntu0.24.04.4"
readonly UBOOT_ELF_SUBPATH="usr/lib/u-boot/qemu-riscv64_smode/uboot.elf"

readonly KERNEL_NAME="7.2.6-1.0_555.1.or"
readonly KERNEL_SHA256="0240645c2f35ea22cf57ea9aaea8481747dfff62e32ac0fe85eeb3767c9968bc"
readonly SERVER_INITRD_SHA256="43ca92d6541f8a44b1d3c1dddbc08d3ae51d7840ff8a62c9f1eaad58ba6186ae"
readonly LABWC_INITRD_SHA256="2ce125c62cd03edad8f910a43d18d774f75839c759e1a5fa7877ede713b515d3"
readonly KDE_INITRD_SHA256="4d9d5d042693f322bde5bc8b771ea89997dcbe30d1a29715dbdc3ada52b3ee8f"

readonly SERVER_CMDLINE="audit=0 root=LABEL=ROOT rw"
readonly WORKSTATION_CMDLINE="loglevel=3 audit=0 rw root=live:CDLABEL=CDROM rd.live.image"

readonly SERVER_ESP_OFFSET=1048576
readonly SERVER_KERNEL_ESP_PATH="openruyi/${KERNEL_NAME}/linux"
readonly SERVER_INITRD_ESP_PATH="openruyi/${KERNEL_NAME}/initrd"
readonly WORKSTATION_KERNEL_ISO_PATH="boot/riscv64/loader/linux"
readonly WORKSTATION_INITRD_ISO_PATH="boot/riscv64/loader/initrd"

usage() {
  echo "Usage: $0 <server|workstation-labwc|workstation-kde> <sbi|u-boot|edk2>" >&2
  echo "       $0 --prepare <server|workstation-labwc|workstation-kde>" >&2
  echo "       $0 --self-test" >&2
}

is_edition() {
  [[ "$1" = server || "$1" = workstation-labwc || "$1" = workstation-kde ]]
}

ACTION=boot
EDITION=""
BOOT_MODE=""
if [[ "${1:-}" = "--self-test" && $# -eq 1 ]]; then
  ACTION=self-test
elif [[ "${1:-}" = "--prepare" && $# -eq 2 ]] && is_edition "$2"; then
  ACTION=prepare
  EDITION=$2
elif (( $# == 2 )) && is_edition "$1" \
  && [[ "$2" = sbi || "$2" = u-boot || "$2" = edk2 ]]; then
  EDITION=$1
  BOOT_MODE=$2
else
  usage
  exit 2
fi

readonly CACHE_DIR="${OPENRUYI_CACHE_DIR:-.cache/openruyi/${RELEASE}}"
readonly WORK_DIR="${OPENRUYI_WORK_DIR:-target/openruyi-boot/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
LOG_FILE="${LOG_DIR}/prototyper-openruyi-${EDITION}-${BOOT_MODE}.log"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${OPENRUYI_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${OPENRUYI_DOWNLOAD_TIMEOUT_SECS:-5400}"
readonly BOOT_TIMEOUT_SECS="${OPENRUYI_BOOT_TIMEOUT_SECS:-1200}"
readonly SMP="${OPENRUYI_SMP:-4}"
readonly MEMORY="${OPENRUYI_MEMORY:-4G}"

if [[ "$BOOT_MODE" = sbi ]]; then
  RUSTSBI_ELF="${OPENRUYI_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
else
  RUSTSBI_ELF="${OPENRUYI_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
fi
readonly RUSTSBI_ELF
readonly QEMU_BIN="${OPENRUYI_QEMU:-qemu-system-riscv64}"

QEMU_PID=""

download_asset() {
  local url=$1
  local destination=$2
  local digest=$3
  local temp="${destination}.part"

  if [[ -f "$destination" ]] && printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status; then
    echo "Using cached $(basename "$destination")" >&2
    return
  fi

  echo "Downloading $url" >&2
  mkdir -p "$(dirname "$destination")"

  local attempt
  for attempt in 1 2 3; do
    if timeout "$DOWNLOAD_TIMEOUT_SECS" aria2c \
      --no-conf=true \
      --connect-timeout="$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
      --timeout=30 --max-tries=5 --retry-wait=2 \
      --split=16 --max-connection-per-server=16 --min-split-size=1M \
      --continue=true --allow-overwrite=true --auto-file-renaming=false \
      --file-allocation=none \
      --dir="$(dirname "$temp")" --out="$(basename "$temp")" \
      "$url"; then
      break
    fi
    if (( attempt == 3 )); then
      return 1
    fi
    echo "Download attempt ${attempt} failed for $url; resuming" >&2
    sleep 10
  done

  if ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check --status; then
    echo "Checksum mismatch for $url" >&2
    rm -f "$temp" "${temp}.aria2"
    return 1
  fi

  mv "$temp" "$destination"
}

verify_sha256() {
  local actual
  actual=$(sha256sum <"$2") || return 1
  [[ "${actual%% *}" = "$1" ]]
}

edition_image_name() {
  case "$EDITION" in
    server) printf '%s\n' "$SERVER_NAME" ;;
    workstation-labwc) printf '%s\n' "$LABWC_NAME" ;;
    workstation-kde) printf '%s\n' "$KDE_NAME" ;;
  esac
}

edition_image_sha256() {
  case "$EDITION" in
    server) printf '%s\n' "$SERVER_SHA256" ;;
    workstation-labwc) printf '%s\n' "$LABWC_SHA256" ;;
    workstation-kde) printf '%s\n' "$KDE_SHA256" ;;
  esac
}

edition_initrd_sha256() {
  case "$EDITION" in
    server) printf '%s\n' "$SERVER_INITRD_SHA256" ;;
    workstation-labwc) printf '%s\n' "$LABWC_INITRD_SHA256" ;;
    workstation-kde) printf '%s\n' "$KDE_INITRD_SHA256" ;;
  esac
}

edition_cmdline() {
  case "$EDITION" in
    server) printf '%s\n' "$SERVER_CMDLINE" ;;
    workstation-*) printf '%s\n' "$WORKSTATION_CMDLINE" ;;
  esac
}

IMAGE_PATH=""
KERNEL_PATH=""
INITRD_PATH=""
UBOOT_ELF_PATH=""
EDK2_VARS_RUN_PATH=""
OVERLAY_PATH=""

prepare_assets() {
  local mode_run_dir="${WORK_DIR}/${EDITION}-${BOOT_MODE}"

  mkdir -p "$CACHE_DIR" "$WORK_DIR" "$mode_run_dir" "$LOG_DIR"
  IMAGE_PATH="${CACHE_DIR}/$(edition_image_name)"
  download_asset "${BASE_URL}/$(edition_image_name)" "$IMAGE_PATH" "$(edition_image_sha256)"

  case "$BOOT_MODE" in
    u-boot)
      UBOOT_ELF_PATH="${mode_run_dir}/uboot.elf"
      if [[ ! -s "$UBOOT_ELF_PATH" ]]; then
        local deb="${CACHE_DIR}/${UBOOT_DEB}"
        download_asset "$UBOOT_URL" "$deb" "$UBOOT_SHA256"
        dpkg-deb -x "$deb" "$mode_run_dir"
        cp "${mode_run_dir}/${UBOOT_ELF_SUBPATH}" "$UBOOT_ELF_PATH"
      fi
      test -s "$UBOOT_ELF_PATH"
      ;;
    edk2)
      download_asset "${BASE_URL}/${EDK2_CODE_NAME}" "${CACHE_DIR}/${EDK2_CODE_NAME}" "$EDK2_CODE_SHA256"
      download_asset "${BASE_URL}/${EDK2_VARS_NAME}" "${CACHE_DIR}/${EDK2_VARS_NAME}" "$EDK2_VARS_SHA256"
      EDK2_VARS_RUN_PATH="${mode_run_dir}/RISCV_VIRT_VARS.fd"
      cp "${CACHE_DIR}/${EDK2_VARS_NAME}" "$EDK2_VARS_RUN_PATH"
      ;;
    sbi)
      KERNEL_PATH="${mode_run_dir}/linux"
      INITRD_PATH="${mode_run_dir}/initrd"
      if [[ ! -s "$KERNEL_PATH" ]] || ! verify_sha256 "$KERNEL_SHA256" "$KERNEL_PATH" \
        || ! verify_sha256 "$(edition_initrd_sha256)" "$INITRD_PATH"; then
        extract_boot_payload
      fi
      verify_sha256 "$KERNEL_SHA256" "$KERNEL_PATH"
      verify_sha256 "$(edition_initrd_sha256)" "$INITRD_PATH"
      ;;
  esac

  if [[ "$EDITION" = server ]]; then
    OVERLAY_PATH="${mode_run_dir}/server.qcow2"
    rm -f "$OVERLAY_PATH"
    qemu-img create -q -f qcow2 -F qcow2 -b "$(realpath "$IMAGE_PATH")" "$OVERLAY_PATH" 20G
  fi
}

extract_boot_payload() {
  local mode_run_dir="${WORK_DIR}/${EDITION}-${BOOT_MODE}"

  if [[ "$EDITION" = server ]]; then
    local disk_raw="${mode_run_dir}/disk.raw"
    timeout --foreground 600 qemu-img convert -f qcow2 -O raw -S 4k "$IMAGE_PATH" "$disk_raw"
    mcopy -i "${disk_raw}@@${SERVER_ESP_OFFSET}" "::/${SERVER_KERNEL_ESP_PATH}" "$KERNEL_PATH"
    mcopy -i "${disk_raw}@@${SERVER_ESP_OFFSET}" "::/${SERVER_INITRD_ESP_PATH}" "$INITRD_PATH"
    rm -f "$disk_raw"
  else
    bsdtar -xf "$IMAGE_PATH" -C "$mode_run_dir" \
      "$WORKSTATION_KERNEL_ISO_PATH" "$WORKSTATION_INITRD_ISO_PATH"
    mv "${mode_run_dir}/${WORKSTATION_KERNEL_ISO_PATH}" "$KERNEL_PATH"
    mv "${mode_run_dir}/${WORKSTATION_INITRD_ISO_PATH}" "$INITRD_PATH"
  fi
}

check_environment() {
  test -s "$RUSTSBI_ELF" || {
    echo "Missing ${RUSTSBI_ELF}; run 'cargo prototyper build' first" >&2
    return 1
  }
  "$QEMU_BIN" --version
  if ! "$QEMU_BIN" -cpu help | awk '$1 == "rva23s64" { found = 1 } END { exit !found }'; then
    echo "${QEMU_BIN} does not provide the required rva23s64 CPU" >&2
    return 1
  fi
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

start_qemu() {
  # shellcheck disable=SC2054 # QEMU keyval options are single comma-joined words.
  local -a machine=(-machine 'virt,acpi=off')
  local -a boot=()
  local -a disks=()

  case "$BOOT_MODE" in
    sbi)
      boot=(-kernel "$(realpath "$KERNEL_PATH")"
        -initrd "$(realpath "$INITRD_PATH")"
        -append "$(edition_cmdline)")
      ;;
    u-boot)
      boot=(-kernel "$(realpath "$UBOOT_ELF_PATH")")
      ;;
    edk2)
      machine=(-machine 'virt,pflash0=pflash0,pflash1=pflash1,acpi=off')
      boot=(
        -blockdev "node-name=pflash0,driver=file,read-only=on,filename=$(realpath "${CACHE_DIR}/${EDK2_CODE_NAME}")"
        -blockdev "node-name=pflash1,driver=file,filename=$(realpath "$EDK2_VARS_RUN_PATH")"
      )
      ;;
  esac

  if [[ "$EDITION" = server ]]; then
    # shellcheck disable=SC2054 # QEMU keyval options are single comma-joined words.
    disks+=(-drive "file=$(realpath "$OVERLAY_PATH"),format=qcow2,id=hd0,if=none"
      -device virtio-blk-device,drive=hd0)
  else
    # shellcheck disable=SC2054 # QEMU keyval options are single comma-joined words.
    disks+=(-blockdev "node-name=iso,driver=file,read-only=on,filename=$(realpath "$IMAGE_PATH")"
      -device virtio-blk-device,drive=iso)
  fi

  : >"$LOG_FILE"
  "$QEMU_BIN" \
    "${machine[@]}" \
    -cpu rva23s64,pmp=true \
    -smp "$SMP" \
    -m "$MEMORY" \
    -nographic \
    -no-reboot \
    -bios "$(realpath "$RUSTSBI_ELF")" \
    "${boot[@]}" \
    "${disks[@]}" \
    -device virtio-rng-device \
    -serial "file:$LOG_FILE" \
    </dev/null >"${LOG_FILE%.log}-qemu.log" 2>&1 &
  QEMU_PID=$!
}

boot_has_failed() {
  grep -aEq 'Kernel panic|not syncing|Attempted to kill init|You are in emergency mode|Entering emergency mode' "$LOG_FILE"
}

userspace_is_ready() {
  local -a markers=('Hello RustSBI!')

  case "$BOOT_MODE" in
    sbi | u-boot)
      markers+=('Redirecting hart [0-9]+ to 0x00000080200000 in Supervisor mode')
      ;;
    edk2)
      markers+=('Redirecting hart [0-9]+ to 0x00000020000000 in Supervisor mode')
      ;;
  esac

  case "$BOOT_MODE" in
    u-boot) markers+=("${UBOOT_BANNER}" 'Booting: Label: virtio' 'GNU GRUB  version 2\.14') ;;
    edk2) markers+=('GNU GRUB  version 2\.14') ;;
    sbi)
      if grep -aqE 'U-Boot 20|GNU GRUB|RISC-V EDK2' "$LOG_FILE"; then
        return 1
      fi
      ;;
  esac

  if [[ "$EDITION" = server ]]; then
    markers+=('Linux version 7\.2\.6-1\.0_555\.1\.or' 'login:[[:space:]]*$')
  else
    markers+=('Booting initrd of ' 'Multi-User System' 'Graphical Interface')
  fi

  local marker
  for marker in "${markers[@]}"; do
    grep -aEq "$marker" "$LOG_FILE" || return 1
  done
}

report_failure() {
  echo "openRuyi ${EDITION} failed to boot (${BOOT_MODE}):" >&2
  grep -a -E --max-count=5 'Kernel panic|not syncing|Attempted to kill init|emergency mode' "$LOG_FILE" >&2 || true
  tail -n 160 "$LOG_FILE" >&2 || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before openRuyi reached userspace (exit=${qemu_exit})" >&2
  tail -n 160 "$LOG_FILE" >&2 || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if boot_has_failed; then
      report_failure
      return 1
    fi
    if userspace_is_ready; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_early_exit
      return 1
    fi
    sleep 1
  done

  echo "openRuyi ${EDITION} did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 160 "$LOG_FILE" >&2 || true
  return 1
}

prepare_downloads() {
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."

  mkdir -p "$CACHE_DIR"
  download_asset "${BASE_URL}/$(edition_image_name)" \
    "${CACHE_DIR}/$(edition_image_name)" "$(edition_image_sha256)"
  download_asset "$UBOOT_URL" "${CACHE_DIR}/${UBOOT_DEB}" "$UBOOT_SHA256"
  download_asset "${BASE_URL}/${EDK2_CODE_NAME}" "${CACHE_DIR}/${EDK2_CODE_NAME}" "$EDK2_CODE_SHA256"
  download_asset "${BASE_URL}/${EDK2_VARS_NAME}" "${CACHE_DIR}/${EDK2_VARS_NAME}" "$EDK2_VARS_SHA256"
}

main() {
  trap stop_qemu EXIT

  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  check_environment
  prepare_assets

  {
    printf 'Boot edition: %s\nBoot mode: %s\n' "$EDITION" "$BOOT_MODE"
    printf 'RustSBI checkout: '
    git rev-parse HEAD 2>/dev/null || echo unknown
    printf 'CI revision: %s\n' "${GITHUB_SHA:-local}"
    "$QEMU_BIN" --version | head -1
    sha256sum "$RUSTSBI_ELF" || true
    printf 'Guest image: %s/%s\n' "$BASE_URL" "$(edition_image_name)"
    printf 'Command line: %s\n' "$(edition_cmdline)"
  } >"${LOG_DIR}/prototyper-openruyi-${EDITION}-${BOOT_MODE}-metadata.log"

  start_qemu
  wait_for_userspace
  stop_qemu

  echo "RustSBI booted openRuyi ${RELEASE} ${EDITION} to userspace (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

run_self_tests() (
  TEST_DIR=$(mktemp -d)
  trap 'rm -rf "$TEST_DIR"' EXIT

  LOG_FILE="$TEST_DIR/serial.log"

  fixture() (
    local edition=$1 mode=$2 variant=${3:-success}
    local ansi=$'\033[1;37m[RustSBI] \033[1;32mINFO \033[0m - '
    {
      printf '%sHello RustSBI!\n' "$ansi"
      if [[ "$variant" = wrong-handoff ]]; then
        printf '%sRedirecting hart 0 to 0x00000010000000 in Supervisor mode.\n' "$ansi"
      else
        if [[ "$mode" = edk2 ]]; then
          printf '%sRedirecting hart 0 to 0x00000020000000 in Supervisor mode.\n' "$ansi"
        else
          printf '%sRedirecting hart 0 to 0x00000080200000 in Supervisor mode.\n' "$ansi"
        fi
      fi
      if [[ "$variant" = edk2-injection ]]; then
        printf 'RISC-V EDK2 firmware version 2.7\n'
      fi
      if [[ "$mode" = u-boot && "$variant" != missing-uboot ]]; then
        printf '%s (Oct 07 2026 - 20:58:59 +0000)\n' "$UBOOT_BANNER"
        [[ "$variant" = missing-efi ]] || printf 'Booting: Label: virtio 0 Device path: /VenHw()\n'
      fi
      if [[ "$mode" = u-boot || "$mode" = edk2 ]]; then
        [[ "$variant" = missing-grub ]] || printf 'GNU GRUB  version 2.14\n'
      fi
      if [[ "$variant" = uboot-injection ]]; then
        printf 'U-Boot 2025.10-0ubuntu0.24.04.4\n'
      fi
      if [[ "$edition" = server ]]; then
        printf '[    0.000000][    T0] Linux version 7.2.6-1.0_555.1.or (abuild@obsworker) #1 SMP\n'
        [[ "$variant" = missing-login ]] || printf 'openruyi-f3b8-a4e3 login: \n'
      else
        printf '\033[0;1;39mBooting initrd of \033[1mopenRuyi Creek dracut-112 (Initramfs)\033[0m.\n'
        [[ "$variant" = missing-initrd-banner ]] || true
        [[ "$variant" = missing-multiuser ]] || printf '[  OK  ] Reached target \033[0;1;39mMulti-User System\033[0m.\n'
        [[ "$variant" = missing-graphical ]] || printf '[  OK  ] Reached target \033[0;1;39mGraphical Interface\033[0m.\n'
      fi
      case "$variant" in
        panic) printf 'Kernel panic - not syncing: No working init found.\n' ;;
        emergency) printf 'You are in emergency mode. After logging in, type "journalctl -xb" to view\n' ;;
      esac
    }
  )

  serial_case() (
    local edition=$1 mode=$2 variant=$3 expected=$4
    EDITION=$edition
    BOOT_MODE=$mode
    fixture "$edition" "$mode" "$variant" >"$LOG_FILE"
    if [[ "$expected" = fail ]]; then
      if boot_has_failed; then
        :
      elif userspace_is_ready; then
        echo "FAIL: $edition/$mode/$variant accepted a bad boot" >&2
        exit 1
      fi
    else
      boot_has_failed && {
        echo "FAIL: $edition/$mode/$variant flagged as fatal" >&2
        exit 1
      }
      userspace_is_ready || {
        echo "FAIL: $edition/$mode/$variant rejected a good boot" >&2
        exit 1
      }
    fi
    echo "PASS: $edition/$mode/$variant ($expected)"
  )

  for edition in server workstation-labwc workstation-kde; do
    for mode in sbi u-boot edk2; do
      serial_case "$edition" "$mode" success pass
    done
  done
  for case_spec in \
    'server sbi panic fail' \
    'server sbi emergency fail' \
    'server sbi wrong-handoff fail' \
    'server sbi uboot-injection fail' \
    'server sbi edk2-injection fail' \
    'server sbi missing-login fail' \
    'server u-boot missing-uboot fail' \
    'server u-boot missing-efi fail' \
    'server u-boot missing-grub fail' \
    'server edk2 missing-grub fail' \
    'workstation-labwc sbi missing-graphical fail' \
    'workstation-labwc sbi missing-multiuser fail' \
    'workstation-labwc sbi panic fail' \
    'workstation-kde u-boot missing-uboot fail' \
    'workstation-kde edk2 missing-grub fail'; do
    # shellcheck disable=SC2086 # Each case is three space-separated words.
    serial_case $case_spec
  done

  for edition in server workstation-labwc workstation-kde; do
    EDITION=$edition
    [[ "$(edition_image_name)" = openRuyi-* ]] || {
      echo "FAIL: bad image name for $edition" >&2
      exit 1
    }
    [[ "$(edition_image_sha256)" =~ ^[0-9a-f]{64}$ ]] || {
      echo "FAIL: bad image digest for $edition" >&2
      exit 1
    }
  done
  echo "PASS: edition asset mapping"

  echo "Offline openRuyi self-tests passed; no guest boot was performed."
)

if [[ "$ACTION" = self-test ]]; then
  run_self_tests
elif [[ "$ACTION" = prepare ]]; then
  prepare_downloads
else
  main
fi
