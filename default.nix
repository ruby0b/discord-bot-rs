{
  yae ? (builtins.fromJSON (builtins.readFile ./yae.json)),
  sources ? {
    nixpkgs = builtins.fetchTarball { inherit (yae.nixpkgs) url sha256; };
    crane = builtins.fetchTarball { inherit (yae.crane) url sha256; };
    fenix = builtins.fetchTarball { inherit (yae.fenix) url sha256; };
  },
  system ? builtins.currentSystem,
  pkgs ? import sources.nixpkgs { inherit system; },
  lib ? pkgs.lib,
  ...
}:
let
  packages = self: {
    fenix = self.callPackage (import sources.fenix) { };
    crane = self.callPackage (import "${sources.crane}/lib") { };
    craneFenix = self.callPackage (
      { crane, fenix }: crane.overrideToolchain fenix.minimal.toolchain
    ) { };
    rstrict = self.callPackage ./nix/rstrict.nix { };
    discord-bot-rs = self.callPackage ./nix/package.nix { };
  };
in
{
  packages = lib.makeScope pkgs.newScope packages;
  nixosModules.default = ./nix/nixos-module.nix;
}
