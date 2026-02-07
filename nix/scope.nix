sources: self: {
  fenix = self.callPackage (import sources.fenix) { };
  crane = self.callPackage (import "${sources.crane}/lib") { };
  craneFenix = self.callPackage (
    { crane, fenix }: crane.overrideToolchain fenix.complete.toolchain
  ) { };
  rstrict = self.callPackage ./rstrict.nix { };
  discord-bot-rs = self.callPackage ./package.nix { };
}
