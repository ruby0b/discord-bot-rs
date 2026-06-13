{
  sources ? import ./npins,
  system ? builtins.currentSystem,
  pkgs ? import sources.nixpkgs { inherit system; },
  lib ? import "${sources.nixpkgs}/lib",
  ...
}:
let
  packages = lib.makeScope pkgs.newScope (self: {
    fenix = self.callPackage (import sources.fenix) { };
    crane = self.callPackage (import "${sources.crane}/lib") { };
    rstrict = self.callPackage ./nix/rstrict.nix { };
    discord-bot-rs = self.callPackage ./nix/discord-bot-rs.nix { };
  });
in
{
  inherit packages;
  devShells.default = pkgs.mkShell {
    # todo: doesn't do what I'd expect, e.g. imagemagick is not in path
    inputsFrom = [ packages.discord-bot-rs ];
    packages = [ packages.fenix.complete.toolchain ];
    env = { inherit (packages.discord-bot-rs) RUSTFLAGS; };
  };
}
