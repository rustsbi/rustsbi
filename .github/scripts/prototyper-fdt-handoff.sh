#!/usr/bin/env bash
# Boot embedded DTBs after excluding firmware mappings, in every firmware mode.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
target_dir=${CARGO_TARGET_DIR:-target}
work_dir="$target_dir/fdt-handoff"
log_dir=qemu-logs
qemu=${QEMU_RISCV64:-qemu-system-riscv64}
mkdir -p "$work_dir" "$log_dir"

# Mark the embedded source so the guest can distinguish it from incoming a1.
source_dtb="$work_dir/source.dtb"
"$qemu" -machine "virt,dumpdtb=$source_dtb" -m 256M -smp 1 -display none
fdtput -t s "$source_dtb" / model rustsbi-dtb-test

linker="$work_dir/probe.ld"
cat >"$linker" <<'EOF'
OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {
  . = 0x80200000;
  .text : { *(.text.entry) . = ALIGN(4); *(.text.trap) *(.text .text.*) }
  .rodata : { *(.rodata .rodata.*) }
  .data : { *(.data .data.*) *(.sdata .sdata.*) }
  .bss : ALIGN(4096) { *(.bss .bss.*) *(.sbss .sbss.*) }
  /DISCARD/ : { *(.eh_frame) }
}
EOF

config="$work_dir/explicit.toml"
{
  printf '%s\n' 'fdt_handoff_address = 0x8fc00000' 'fdt_handoff_capacity = 65536'
  cat firmware/prototyper/config/default.toml
} >"$config"

for name in dynamic-incoming dynamic-explicit jump-explicit payload-explicit; do
  mode=${name%-*}
  guest="$work_dir/$name.elf"
  binary="$work_dir/$name.bin"
  (
    unset FDT_EXPLICIT
    if [[ "$name" == *-explicit ]]; then
      export FDT_EXPLICIT=1
    fi
    rustc --edition=2024 --target=riscv64imac-unknown-none-elf -Cpanic=abort \
      -Copt-level=2 "-Clink-arg=-T$linker" -Clink-arg=--no-relax \
      .github/scripts/fdt-handoff.rs -o "$guest"
  )
  rust-objcopy -O binary "$guest" "$binary"

  build_args=(cargo prototyper build --fdt "$source_dtb")
  if [[ "$name" == *-explicit ]]; then
    build_args+=(--config-file "$config")
  fi
  build_args+=("$mode")
  extra_args=()
  case "$mode" in
    dynamic) extra_args=(-kernel "$binary") ;;
    jump) extra_args=(-device "loader,file=$binary,addr=0x80200000") ;;
    payload) build_args+=("$binary") ;;
  esac
  "${build_args[@]}"

  bios="$target_dir/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-$mode.elf"
  log_file="$log_dir/fdt-handoff-$name.log"
  if timeout 30s "$qemu" -machine virt -m 256M -smp 1 -nographic \
    -bios "$bios" "${extra_args[@]}" >"$log_file" 2>&1; then
    qemu_exit=0
  else
    qemu_exit=$?
  fi
  if [[ "$qemu_exit" != 0 ]] || ! grep -Fq 'FDT handoff: PASS' "$log_file"; then
    echo "[$name] QEMU exit: $qemu_exit; log: $log_file" >&2
    cat "$log_file" >&2
    exit 1
  fi
  echo "$name: PASS (DTB read after firmware mappings were excluded)"
done
