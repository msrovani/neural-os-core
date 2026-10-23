# JARBAS kiosk on iGPU via cage (Wayland) — appliance HUD, sem DM.
{ config, lib, pkgs, ... }:
let
  cfg = config.services.jarbasCage;
in
{
  options.services.jarbasCage = {
    enable = lib.mkEnableOption "JARBAS HUD kiosk (cage + nos-jarbas)";
    nosdPackage = lib.mkOption {
      type = lib.types.package;
      default = pkgs.nosd or pkgs.hello;
      description = "Package providing nos-jarbas binary";
    };
    # Bind compositor to iGPU only — never steal dGPU VRAM
    drmDevice = lib.mkOption {
      type = lib.types.str;
      default = "/dev/dri/card0";
      description = "iGPU DRM node for WLR_DRM_DEVICES";
    };
    tty = lib.mkOption {
      type = lib.types.str;
      default = "tty1";
    };
  };

  config = lib.mkIf cfg.enable {
    services.seatd.enable = true;

    # Nosd user needs video/render + seat for DRM
    users.users.nosd.extraGroups = lib.mkAfter [ "video" "render" "seat" ];

    systemd.tmpfiles.rules = [
      "d /run/user/nosd 0700 nosd nosd -"
    ];

    environment.systemPackages = with pkgs; [ cage cfg.nosdPackage ];

    # Libera tty1 do getty quando cage sobe
    systemd.services."getty@${cfg.tty}" = {
      enable = lib.mkForce false;
    };

    systemd.services.jarbas-cage = {
      description = "JARBAS Wayland kiosk (iGPU)";
      wantedBy = [ "graphical.target" ];
      after = [ "nosd.service" "seatd.service" "systemd-user-sessions.service" ];
      wants = [ "nosd.service" "seatd.service" ];
      serviceConfig = {
        Type = "simple";
        User = "nosd";
        Group = "nosd";
        PAMName = "login";
        TTYPath = "/dev/${cfg.tty}";
        TTYReset = "yes";
        TTYVHangup = "yes";
        StandardInput = "tty";
        StandardOutput = "journal";
        StandardError = "journal";
        Environment = [
          "WLR_DRM_DEVICES=${cfg.drmDevice}"
          "XDG_RUNTIME_DIR=/run/user/nosd"
          "XDG_SESSION_TYPE=wayland"
          "NOSD_SOCKET=/run/nosd/nosd.sock"
        ];
        # Cage kiosk; nos-jarbas = HUD client (scaffold chat até Slint)
        ExecStart = "${pkgs.cage}/bin/cage -s -- ${cfg.nosdPackage}/bin/nos-jarbas --socket /run/nosd/nosd.sock";
        Restart = "on-failure";
        RestartSec = "3s";
      };
    };
  };
}
