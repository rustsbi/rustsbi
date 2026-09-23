# NixOS boot coverage

Boots a minimal riscv64 NixOS guest on QEMU `virt` through the dynamic
Prototyper firmware: Linux → NixOS stage 1 (netboot initrd with the full
system closure) → stage 2 systemd. `flake.lock` pins the nixos-unstable
nixpkgs revision; the boot needs no disk, network, or host store mount.

## Run

Requires Nix with flakes, `qemu-system-riscv64`, and the Rust toolchain
from `rust-toolchain.toml` plus cargo-binutils.

```sh
nix build path:./.github/nixos -o target/nixos-boot-assets \
  --option substituters "https://cache.nixos.org https://cache.ztier.in" \
  --option trusted-public-keys "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.ztier.link-1:3P5j2ZB9dNgFFFVkCQWT3mh0E+S3rIWtZvoql64UaXM="
cargo prototyper build
.github/scripts/prototyper-nixos-boot.sh
```

`cache.ztier.in` is a community cache for riscv64 paths on nixos-unstable
(see the [NixOS RISC-V wiki](https://wiki.nixos.org/wiki/RISC-V)); without
it the cross kernel builds locally. Refresh the image with
`nix flake update nixpkgs` run in this directory.

## Pass criteria

A stage-2 systemd service checks the riscv64 architecture, NixOS identity,
systemd PID 1, `/run/current-system`, the mounted `/nix/store`, and an
active journald, then prints `RUSTSBI-NIXOS-BOOT-OK` and powers off. The
host script fails on panics, a missing marker, or a timeout; serial logs
are kept in `qemu-logs/`.

The workflow builds the assets through Nix only on an `actions/cache` miss
keyed by this directory, and always rebuilds RustSBI from the commit under
test.
