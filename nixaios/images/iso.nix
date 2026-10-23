# Offline MINIMAL ISO — nosd + JARBAS cage kiosk (sem GNOME/KDE).
# Manifesto: appliance HUD; iGPU=card0 para cage; dGPU livre para tensores.
# Build: nix build .#iso
{ pkgs, lib, modulesPath, ... }:
{
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix"
  ];

  isoImage.makeEfiBootable = true;
  isoImage.makeUsbBootable = true;
  isoImage.volumeID = "NIXAIOS";

  networking.hostName = "nixaios";
  networking.networkmanager.enable = true;
  # installation-device.nix defaults SSH on; force off for offline live ISO
  services.openssh.enable = lib.mkForce false;

  # Mesa/Vulkan — sem NVIDIA proprietary na ISO live (opt-in pós-install)
  hardware.graphics.enable = true;
  services.xserver.videoDrivers = lib.mkDefault [ "modesetting" ];

  # Evita agetty competir com cage no tty1
  services.getty.autologinUser = lib.mkForce null;

  services.nosd = {
    enable = true;
    dataDir = "/var/lib/nosd";
  };

  # JARBAS kiosk Wayland — iGPU only (WLR_DRM_DEVICES=card0)
  services.jarbasCage = {
    enable = true;
    nosdPackage = pkgs.nosd;
    drmDevice = "/dev/dri/card0";
    tty = "tty1";
  };

  # CapGate NIX_WRITE staging allowlist (HITL rebuild)
  services.nosdAiManaged = {
    enable = true;
  };

  system.activationScripts.nosdModels = ''
    mkdir -p /var/lib/nosd/models
    mkdir -p /var/lib/nosd/nsgdb
    mkdir -p /run/user/nosd
    chown nosd:nosd /run/user/nosd || true
    chmod 700 /run/user/nosd || true
  '';

  environment.systemPackages = with pkgs; [
    pciutils
    vulkan-tools
    vulkan-loader
    mesa
    cage
    nosd
  ];

  # Offline-friendly: no channels fetch at boot
  nix.settings.substituters = lib.mkDefault [ ];

  # graphical.target para cage; multi-user já sobe nosd
  systemd.defaultUnit = lib.mkOverride 900 "graphical.target";
}
