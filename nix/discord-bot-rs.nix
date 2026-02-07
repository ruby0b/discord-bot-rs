{
  craneFenix,
  d2,
  imagemagick,
  lib,
  libopus,
  pkg-config,
  rstrict,
  typst,
  makeWrapper,
}:
let
  main = "discord-bot-rs";
in
craneFenix.buildPackage {
  pname = main;
  src = lib.cleanSourceWith {
    src = ../.;
    name = "source";
    filter =
      path: type: (builtins.match ".*src/.*" path != null) || (craneFenix.filterCargoSources path type);
  };
  strictDeps = true;
  doCheck = false;
  nativeBuildInputs = [
    makeWrapper
    pkg-config
  ];
  buildInputs = [ libopus ];
  # https://github.com/nix-community/fenix/issues/191
  env.RUSTFLAGS = "-Clinker-features=-lld";
  postInstall = ''
    wrapProgram $out/bin/${main} --prefix PATH : ${
      lib.makeBinPath [
        d2
        imagemagick
        typst
        rstrict
      ]
    }
  '';
  meta.mainProgram = main;
}
