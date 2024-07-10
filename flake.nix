{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      forAllSystems = function: nixpkgs.lib.genAttrs
        [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ]
        (system: function (import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        }));

      discord-bot-rs = { cmake, lib, makeRustPlatform, openssl, pkg-config, rust-bin, ... }:
        let
          nightly = rust-bin.selectLatestNightlyWith (toolchain: toolchain.default);
          rustPlatform = makeRustPlatform { cargo = nightly; rustc = nightly; };
        in
        rustPlatform.buildRustPackage rec {
          pname = "discord-bot-rs";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false; # no tests, compiling takes long enough as is
          nativeBuildInputs = [ cmake pkg-config ];
          buildInputs = [ openssl ];
          PKG_CONFIG_PATH = "${openssl.dev}/lib/pkgconfig";
          meta.mainProgram = pname;
        };
    in
    {
      packages = forAllSystems (pkgs:
        let exe = pkgs.callPackage discord-bot-rs { }; in {
          default = exe;
          discord-bot-rs = exe;
        });

      overlays = {
        default = (final: prev: {
          discord-bot-rs = final.callPackage discord-bot-rs { };
        });
        rust-overlay = rust-overlay.overlays.default;
      };

      nixosModules.default = { lib, config, pkgs, ... }:
        let
          inherit (lib) mkOption types mkIf;
          name = "discord-bot-rs";
          cfg = config.services.${name};
        in
        {
          options.services.${name} = {
            enable = mkOption { type = types.bool; default = false; };
            autostart = mkOption { type = types.bool; default = true; };
            token = mkOption { type = types.uniq types.str; };
          };
          config = mkIf cfg.enable {
            users.users.${name} = {
              description = name;
              home = "/var/lib/${name}";
              createHome = true;
              isNormalUser = true;
              group = name;
            };
            users.groups.${name} = { };
            systemd.services.${name} = {
              description = name;
              wantedBy = if cfg.autostart then [ "multi-user.target" ] else [ ];
              after = [ "network-online.target" ];
              wants = [ "network-online.target" ];
              environment = { DISCORD_TOKEN = cfg.token; };
              serviceConfig = {
                ExecStart = "${pkgs.lib.getExe pkgs.${name}}";
                WorkingDirectory = "/var/lib/${name}";
                User = name;
                Restart = "always";
                RestartSec = 5;
              };
            };
          };
        };
    };
}
