{
  description = "Minimal NixOS guest for RustSBI QEMU boot coverage";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      hostSystem = "x86_64-linux";
      hostPkgs = import nixpkgs { system = hostSystem; };
      guest = nixpkgs.lib.nixosSystem {
        modules = [
          ./guest.nix
          {
            nixpkgs.buildPlatform = hostSystem;
            nixpkgs.hostPlatform = "riscv64-linux";
          }
        ];
      };
    in {
      nixosConfigurations.rustsbi = guest;
      packages.${hostSystem}.default = hostPkgs.runCommand "rustsbi-nixos-boot-assets" { } ''
        mkdir -p "$out"
        cp ${guest.config.system.build.kernel}/${guest.config.system.boot.loader.kernelFile} "$out/Image"
        cp ${guest.config.system.build.netbootRamdisk}/initrd "$out/initrd"
        printf '%s\n' 'init=${guest.config.system.build.toplevel}/init ${toString guest.config.boot.kernelParams}' > "$out/cmdline"
        cd "$out"
        sha256sum Image initrd cmdline > SHA256SUMS
      '';
    };
}
