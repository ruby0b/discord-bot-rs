{ }:
{
  lib,
  config,
  options,
  ...
}:
let
  inherit (lib) mkOption types;
  inherit (config.package) pname;
in
{
  _class = "service";

  options = {
    autostart = mkOption {
      type = types.bool;
      default = true;
    };
    # XXX: can't use finalAttrs.finalPackage: https://github.com/ipetkov/crane/issues/963
    package = lib.mkOption { type = lib.types.package; };
    token = mkOption { type = types.uniq types.str; };
    config_url = mkOption { type = types.uniq types.str; };
    serpapi_token = mkOption { type = types.uniq types.str; };
  };

  config = {
    process.argv = [
      (lib.getExe config.package)
      "--hide-window"
      "--service-mode"
    ];
  }
  // lib.optionalAttrs (options ? systemd) {
    systemd.service = {
      wantedBy = if config.autostart then [ "multi-user.target" ] else [ ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      environment = {
        HOME = "/var/lib/${pname}";
        BOT_TOKEN = config.token;
        BOT_CONFIG_CHANNEL = config.config_url;
        SERPAPI_TOKEN = config.serpapi_token;
      };
      serviceConfig = {
        Restart = "always";
        RestartSec = 30;
        DynamicUser = true;
        StateDirectory = pname;
        WorkingDirectory = "/var/lib/${pname}";

        # Hardening https://blog.sergeantbiggs.net/posts/hardening-applications-with-systemd/
        ## File System
        PrivateDevices = true; # Only allow access to pseudo-devices (eg: null, random, zero) in separate namespace
        ProtectKernelLogs = true; # Disable access to Kernel Logs
        ProtectProc = "invisible"; # Disable access to information about other processes
        PrivateUsers = true; # Disable access to other users on system
        ProtectHome = true; # Disable access to /home
        UMask = "0077"; # set umask
        ## System
        RestrictNamespaces = true; # Disable creating namespaces
        LockPersonality = true; # Locks personality system call
        NoNewPrivileges = true; # Service may not acquire new privileges
        ProtectKernelModules = true; # Service may not load kernel modules
        SystemCallArchitectures = "native"; # Only allow native system calls
        ProtectHostname = true; # Service may not change host name
        RestrictAddressFamilies = "AF_INET AF_INET6"; # Service may only use IP address families
        RestrictRealtime = true; # Disable realtime privileges
        ProtectControlGroups = true; # Disable access to cgroups
        ProtectKernelTunables = true; # Disable write access to kernel variables
        RestrictSUIDSGID = true; # Disable setting suid or sgid bits
        ProtectClock = true; # Disable changing system clock
        MemoryDenyWriteExecute = true; # Disable writing to executable memory (semi-useless if fs isn't noexec)
        ## Capabilities and syscalls
        CapabilityBoundingSet = ""; # Disable all capabilities
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
          "@resources"
        ];
      };
    };
  };
}
