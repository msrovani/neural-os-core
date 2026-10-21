# NixOS module — nosd systemd Type=notify (ADR-0118)
{ config, lib, pkgs, ... }:
let
  cfg = config.services.nosd;
in
{
  options.services.nosd = {
    enable = lib.mkEnableOption "Neural-OS Daemon (nosd)";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.nosd or pkgs.hello;
      description = "nosd package (flake should set pkgs.nosd)";
    };
    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/nosd";
    };
    socket = lib.mkOption {
      type = lib.types.str;
      default = "/run/nosd/nosd.sock";
    };
    nodeId = lib.mkOption {
      type = lib.types.ints.u32;
      default = 15;
    };
    # Exclude iGPU from CUDA/HIP compute (display stays on card0)
    cudaVisibleDevices = lib.mkOption {
      type = lib.types.str;
      default = "0";
      description = "CUDA_VISIBLE_DEVICES for dGPU compute";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.nosd = {
      isSystemUser = true;
      group = "nosd";
      extraGroups = [ "video" "render" ];
    };
    users.groups.nosd = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0750 nosd nosd -"
      "d ${cfg.dataDir}/nsgdb 0750 nosd nosd -"
      "d ${cfg.dataDir}/models 0755 nosd nosd -"
      "d /run/nosd 0755 nosd nosd -"
    ];

    # HugePages hint (operator may raise)
    boot.kernel.sysctl."vm.nr_hugepages" = lib.mkDefault 256;

    systemd.services.nosd = {
      description = "Neural-OS Daemon";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      serviceConfig = {
        Type = "notify";
        User = "nosd";
        Group = "nosd";
        ExecStart = "${cfg.package}/bin/nosd --foreground --socket ${cfg.socket} --data-dir ${cfg.dataDir} --node-id ${toString cfg.nodeId}";
        Restart = "on-failure";
        RestartSec = "2s";
        Nice = -10;
        CPUWeight = 800;
        MemoryMax = "16G";
        RuntimeDirectory = "nosd";
        StateDirectory = "nosd";
        # DRM render nodes for dGPU compute
        DeviceAllow = [ "char-drm rw" ];
        RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ cfg.dataDir "/run/nosd" ];
        Environment = [
          "NOSD_ENGINE_DIR=${cfg.package}/lib/nosd/engines"
          "CUDA_VISIBLE_DEVICES=${cfg.cudaVisibleDevices}"
          "RUST_LOG=info"
        ];
      };
    };
  };
}
