{
  alsa-lib,
  cmake,
  craneLib,
  d2,
  imagemagick,
  lib,
  libopus,
  openssl,
  pkg-config,
  rstrict,
  typst,
  makeWrapper,
}:
let
  main = "discord-bot-rs";
in
craneLib.buildPackage {
  pname = main;
  src = lib.cleanSourceWith {
    src = ../.;
    name = "source";
    filter =
      path: type: (builtins.match ".*src/.*" path != null) || (craneLib.filterCargoSources path type);
  };
  strictDeps = true;
  doCheck = false;
  nativeBuildInputs = [
    cmake
    pkg-config
    makeWrapper
  ];
  buildInputs = [
    alsa-lib
    libopus
    openssl
    d2
    imagemagick
    rstrict
    typst
  ];
  env = {
    # https://github.com/nix-community/fenix/issues/191
    RUSTFLAGS = "-Zlinker-features=-lld";
  };
  postInstall = ''
    wrapProgram $out/bin/${main} \
      --prefix-each PATH : ${d2}/bin \
      --prefix PATH : ${imagemagick}/bin \
      --prefix PATH : ${typst}/bin \
      --prefix PATH : ${rstrict}/bin \
      --prefix PKG_CONFIG_PATH : ${openssl.dev}/lib/pkgconfig \
      --prefix PKG_CONFIG_PATH : ${alsa-lib.dev}/lib/pkgconfig
  '';
  meta.mainProgram = main;
}
