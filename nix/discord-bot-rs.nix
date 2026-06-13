{
  crane,
  fenix,
  d2,
  imagemagick,
  lib,
  libopus,
  makeWrapper,
  pkg-config,
  rstrict,
  typst,
}:
let
  main = "discord-bot-rs";
  craneFenix = crane.overrideToolchain fenix.minimal.toolchain;
in
craneFenix.buildPackage {
  pname = main;
  src = lib.cleanSourceWith {
    src = ../.;
    filter = crane.filterCargoSources;
  };

  strictDeps = true;
  doCheck = false;

  nativeBuildInputs = [
    makeWrapper
    pkg-config
  ];
  buildInputs = [ libopus ];
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

  passthru.services.default = lib.modules.importApply ./service.nix { };
  meta.mainProgram = main;
}
