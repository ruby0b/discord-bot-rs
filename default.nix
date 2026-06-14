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
    inputsFrom = [ packages.discord-bot-rs ];
    packages = packages.discord-bot-rs.runtime-dependencies ++ [
      packages.fenix.complete.toolchain
    ];
    env = { inherit (packages.discord-bot-rs) RUSTFLAGS; };
  };
}
