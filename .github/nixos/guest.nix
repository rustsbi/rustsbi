{ lib, pkgs, modulesPath, ... }:
{
  imports = [
    "${modulesPath}/profiles/minimal.nix"
    "${modulesPath}/installer/netboot/netboot.nix"
  ];

  system.stateVersion = "26.05";
  networking.hostName = "rustsbi-nixos";
  networking.useDHCP = false;
  networking.firewall.enable = false;
  services.udisks2.enable = false;
  services.getty.autologinUser = null;
  security.sudo.enable = false;
  nix.enable = false;
  # A boot test has no need to initialise the Nix package database.
  boot.postBootCommands = lib.mkForce "";
  boot.kernelParams = [ "console=ttyS0,115200" "panic=-1" ];
  boot.initrd.availableKernelModules = [ "virtio_mmio" "virtio_blk" ];
  boot.initrd.systemd.enable = false;
  netboot.squashfsCompression = "zstd -Xcompression-level 3";

  systemd.services.rustsbi-smoke = {
    description = "Verify NixOS stage 2 under RustSBI";
    wantedBy = [ "multi-user.target" ];
    requires = [ "local-fs.target" ];
    after = [ "local-fs.target" "systemd-journald.service" ];
    path = [ pkgs.coreutils pkgs.gnugrep pkgs.util-linux pkgs.systemd ];
    serviceConfig = {
      Type = "oneshot";
      StandardOutput = "tty";
      StandardError = "tty";
      TTYPath = "/dev/ttyS0";
    };
    script = ''
      set -eu
      test "$(uname -m)" = riscv64
      . /etc/os-release
      test "$ID" = nixos
      test "$(cat /proc/1/comm)" = systemd
      test -e /run/current-system
      test -d /nix/store
      mountpoint -q /nix/store
      systemctl is-active --quiet systemd-journald.service
      echo "RUSTSBI-NIXOS-BOOT-OK version=$VERSION_ID kernel=$(uname -r)"
      systemctl --no-block poweroff
    '';
  };
}
