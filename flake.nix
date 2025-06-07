{
  inputs.crane.url = "github:ipetkov/crane";
  inputs.fenix.url = "github:nix-community/fenix/monthly";

  outputs =
    { crane, fenix, ... }:
    let
      nixpkgs = fenix.inputs.nixpkgs;
      fenix-overlay = (
        _: super:
        let
          pkgs = fenix.inputs.nixpkgs.legacyPackages.${super.system};
        in
        fenix.overlays.default pkgs pkgs
      );
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed (
          system:
          function (
            import nixpkgs {
              inherit system;
              overlays = [ fenix.overlays.default ];
            }
          )
        );
      overlay =
        final: prev:
        let
          rstrict = final.callPackage ./nix/rstrict.nix { };
        in
        {
          inherit rstrict;
          discord-bot-rs = final.callPackage ./nix/package.nix {
            craneLib = (crane.mkLib final).overrideToolchain final.fenix.minimal.toolchain;
            inherit rstrict;
          };
        };
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          overlayed = overlay pkgs pkgs;
        in
        overlayed // { default = overlayed.discord-bot-rs; }
      );
      overlays.default = overlay;
      overlays.fenix = fenix-overlay;
      nixosModules.default = ./nix/nixos-module.nix;
    };
}
