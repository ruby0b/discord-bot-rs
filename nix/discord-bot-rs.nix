{
  crane,
  fenix,
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
  runtime-dependencies = [
    typst
    rstrict
  ];
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
    wrapProgram $out/bin/${main} --prefix PATH : ${lib.makeBinPath runtime-dependencies}
  '';

  passthru = {
    inherit runtime-dependencies;
    services.default = lib.modules.importApply ./service.nix { };
  };
  meta.mainProgram = main;
}
