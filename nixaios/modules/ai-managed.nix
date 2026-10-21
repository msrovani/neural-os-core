# AI-managed Nix fragment — CapGate NIX_WRITE + HITL switch
{ config, lib, pkgs, ... }:
let
  cfg = config.services.nosdAiManaged;
in
{
  options.services.nosdAiManaged = {
    enable = lib.mkEnableOption "Import ai-managed.nix allowlist for Cortex proposals";
    path = lib.mkOption {
      type = lib.types.path;
      default = "/etc/nixos/ai-managed.nix";
      description = "Only file nosd may propose writes into (Landlock/allowlist)";
    };
  };

  config = lib.mkIf cfg.enable {
    # Empty default — IA writes staging, never this path directly without HITL
    environment.etc."nixos/ai-managed.nix" = {
      text = ''
        # Managed by nosd CapGate NIX_WRITE after build-vm + HITL approve.
        # Do not edit by hand unless you know the generation rollback path.
        { ... }: { }
      '';
      mode = "0640";
      user = "root";
      group = "nosd";
    };

    # Helper script: staging → build-vm → (human) switch
    environment.systemPackages = [
      (pkgs.writeShellScriptBin "nosd-nix-propose-check" ''
        set -euo pipefail
        STAGING="''${1:-/var/lib/nosd/staging-ai-managed.nix}"
        if [ ! -f "$STAGING" ]; then
          echo "missing staging $STAGING" >&2
          exit 1
        fi
        # Validate by building a VM (does not switch the host)
        nixos-rebuild build-vm -I nixos-config="$STAGING"
        echo "build-vm OK — HITL required before: nixos-rebuild switch"
        echo "rollback: nixos-rebuild --rollback"
      '')
    ];
  };
}
