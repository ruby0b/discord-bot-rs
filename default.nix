{
  system ? builtins.currentSystem,
  yae ? (builtins.fromJSON (builtins.readFile ./yae.json)),
  sources ? {
    nixpkgs = builtins.fetchTarball { inherit (yae.nixpkgs) url sha256; };
    crane = builtins.fetchTarball { inherit (yae.crane) url sha256; };
    fenix = builtins.fetchTarball { inherit (yae.fenix) url sha256; };
  },
  fenix-overlay ? import "${sources.fenix}/overlay.nix",
  pkgs ? import sources.nixpkgs {
    inherit system;
    overlays = [ fenix-overlay ];
  },
  ...
}:
let
  overlay =
    final: _:
    let
      # todo is this crane import idiomatic in an overlay?
      crane = import sources.crane { pkgs = final; };
      rstrict = final.callPackage ./nix/rstrict.nix { };
      discord-bot-rs = final.callPackage ./nix/package.nix {
        craneLib = crane.overrideToolchain final.fenix.minimal.toolchain;
        # todo this won't use overlayed rstrict changes, research makeScope
        inherit rstrict;
      };
    in
    {
      inherit discord-bot-rs rstrict;
    };
in
{
  packages = overlay pkgs null;
  overlays.default = overlay;
  overlays.fenix = fenix-overlay;
  nixosModules.default = ./nix/nixos-module.nix;
}
