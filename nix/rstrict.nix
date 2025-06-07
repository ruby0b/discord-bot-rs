{
  fetchCrate,
  rustPlatform,
}:
rustPlatform.buildRustPackage rec {
  pname = "rstrict";
  version = "0.1.14";

  src = fetchCrate {
    inherit pname version;
    hash = "sha256-OK0BKWzNiastSOKxOYCoQ+ddoYy4yc8tlc/acFuY8MU=";
  };

  cargoLock.lockFile = "${src}/Cargo.lock";

  meta.mainProgram = "rstrict";
}
