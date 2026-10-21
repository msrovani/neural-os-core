# JARBAS kiosk on iGPU via cage (Wayland)
{ config, lib, pkgs, ... }:
let
  cfg = config.services.jarbasCage;
in
{
  options.services.jarbasCage = {
    enable = lib.mkEnableOption "JARBAS HUD kiosk (cage + nos-jarbas)";
    nosdPackage = lib.mkOption {
      type = lib.types.package;
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

    environment.systemPackages = with pkgs; [ cage ];

    systemd.services.jarbas-cage = {
      description = "JARBAS Wayland kiosk (iGPU)";
      wantedBy = [ "graphical.target" ];
      after = [ "nosd.service" "seatd.service" ];
      requires = [ "nosd.service" ];
      serviceConfig = {
        Type = "simple";
        User = "nosd";
        Group = "nosd";
        PAMName = "login";
        TTYPath = "/dev/${cfg.tty}";
        StandardInput = "tty";
        StandardOutput = "journal";
        Environment = [
          "WLR_DRM_DEVICES=${cfg.drmDevice}"
          "XDG_RUNTIME_DIR=/run/user/nosd"
          "NOSD_SOCKET=/run/nosd/nosd.sock"
        ];
        # cage runs a single client full-screen; Slint UI replaces echo when ready
        ExecStart = "${pkgs.cage}/bin/cage -- ${cfg.nosdPackage}/bin/nos-jarbas --socket /run/nosd/nosd.sock";
        Restart = "on-failure";
      };
    };
  };
}
