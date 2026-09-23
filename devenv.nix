{ config, pkgs, ... }:
{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
    lsp.enable = false;
  };

  packages = with pkgs; [
    cargo-binutils
    qemu
    dtc
    ubootTools
    coreutils
    gnugrep
    bash
    curl
    git
    nix
    shellcheck
  ];

  # Keep Cargo downloads and any accidental rustup invocation project-local.
  env.CARGO_HOME = "${config.devenv.state}/cargo";
  env.RUSTUP_HOME = "${config.devenv.state}/rustup";

  scripts.nixos-build-assets.exec = ''
    mkdir -p target
    nix build path:./.github/nixos -o target/nixos-boot-assets \
      --option substituters "https://cache.nixos.org https://cache.ztier.in" \
      --option trusted-public-keys "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= cache.ztier.link-1:3P5j2ZB9dNgFFFVkCQWT3mh0E+S3rIWtZvoql64UaXM=" \
      --max-jobs 2 --cores 6 "$@"
  '';
  scripts.nixos-boot.exec = ''
    cargo prototyper build
    exec bash .github/scripts/prototyper-nixos-boot.sh "$@"
  '';
}
