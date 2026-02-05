{
  yae ? (builtins.fromJSON (builtins.readFile ./yae.json)),
  sources ? {
    nixpkgs = builtins.fetchTarball { inherit (yae.nixpkgs) url sha256; };
    nixpkgs-lib = builtins.fetchTarball { inherit (yae.nixpkgs-lib) url sha256; };
    crane = builtins.fetchTarball { inherit (yae.crane) url sha256; };
    fenix = builtins.fetchTarball { inherit (yae.fenix) url sha256; };
  },
  system ? builtins.currentSystem,
  pkgs ? import sources.nixpkgs { inherit system; },
  lib ? import "${sources.nixpkgs-lib}/lib",
  ...
}:
let
  makePackages = pkgs: lib.makeScope pkgs.newScope (import ./nix/scope.nix sources);
in
{
  packages = makePackages pkgs;
  nixosModules.default = import ./nix/nixos-module.nix makePackages;
}
