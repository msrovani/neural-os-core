# Offline ISO recipe — drivers + nosd + models dir
# Build: nix build .#nixosConfigurations.nixaios-iso.config.system.build.isoImage
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
  services.openssh.enable = lib.mkDefault false;

  # GPU stacks (unfree NVIDIA optional)
  hardware.graphics.enable = true;
  services.xserver.videoDrivers = lib.mkDefault [ "modesetting" ];

  # Enable nosd on the live ISO (package via flake overlay → pkgs.nosd)
  services.nosd = {
    enable = true;
    dataDir = "/var/lib/nosd";
  };

  # Pre-create models mount point; operator copies GGUF into ISO overlay / nixos-generators files
  system.activationScripts.nosdModels = ''
    mkdir -p /var/lib/nosd/models
    mkdir -p /var/lib/nosd/nsgdb
  '';

  environment.systemPackages = with pkgs; [
    pciutils
    vulkan-tools
    cage
  ];

  # Offline-friendly: no channels fetch at boot
  nix.settings.substituters = lib.mkDefault [ ];
}
