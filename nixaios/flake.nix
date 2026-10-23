{
  description = "NixAIOS — nosd daemon + JARBAS kiosk on NixOS (ADR-0118)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    nixos-generators.url = "github:nix-community/nixos-generators";
    nixos-generators.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, nixos-generators, ... }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
            config.allowUnfree = true;
          };
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" ];
          };
          nosdPkg = pkgs.rustPlatform.buildRustPackage {
            pname = "nosd";
            version = "0.1.0";
            src = ./packages/nosd;
            cargoLock.lockFile = ./packages/nosd/Cargo.lock;
            nativeBuildInputs = with pkgs; [
              pkg-config
              rust
            ];
            buildInputs = with pkgs; [
              openssl
              libgcc
            ] ++ pkgs.lib.optionals pkgs.config.cudaSupport [
              pkgs.cudaPackages.cudatoolkit
            ];
            # RPATH for future CUDA/ROCm engines under $out/lib/nosd/engines
            postFixup = ''
              mkdir -p $out/lib/nosd/engines
              if [ -d "$out/bin" ]; then
                for b in $out/bin/*; do
                  if [ -f "$b" ]; then
                    patchelf --set-rpath "${pkgs.lib.makeLibraryPath [ pkgs.libgcc pkgs.openssl ]}:$out/lib/nosd/engines" "$b" || true
                  fi
                done
              fi
            '';
            meta = {
              description = "Neural-OS Daemon (NixAIOS)";
              license = pkgs.lib.licenses.agpl3Only;
              mainProgram = "nosd";
            };
          };
        in
        {
          nosd = nosdPkg;
          default = nosdPkg;
          # Default nixpkgs llama-cpp = CPU. CUDA build is heavy / unfree — opt-in:
          #   nix build .#llama-cpp-cuda
          # Then: NOSD_LLAMA_CLI=result/bin/llama-completion NOSD_LLAMA_NGL=99
          # Forward GPU = VERIFIED só com system_info CUDA + offload (não basta nvidia-smi).
          llama-cpp-cuda = pkgs.llama-cpp.override { cudaSupport = true; };
        } // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # Convenience: nix build .#iso (from WSL/Linux)
          iso = self.nixosConfigurations.nixaios-iso.config.system.build.isoImage;
        });

      nixosModules.neural-os = import ./modules/neural-os.nix;
      nixosModules.jarbas-cage = import ./modules/jarbas-cage.nix;
      nixosModules.ai-managed = import ./modules/ai-managed.nix;
      nixosModules.default = { ... }: {
        imports = [
          self.nixosModules.neural-os
          self.nixosModules.jarbas-cage
          self.nixosModules.ai-managed
        ];
      };

      nixosConfigurations.nixaios-iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [
          ({ pkgs, ... }: {
            nixpkgs.config.allowUnfree = true;
            nixpkgs.overlays = [
              (final: prev: {
                nosd = self.packages.x86_64-linux.nosd;
              })
            ];
          })
          self.nixosModules.default
          ./images/iso.nix
          ({ pkgs, ... }: {
            services.nosd.package = pkgs.nosd;
            environment.systemPackages = [ pkgs.nosd ];
          })
        ];
      };

      devShells = forAllSystems (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; config.allowUnfree = true; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              (rust-bin.stable.latest.default.override { extensions = [ "rust-src" "clippy" "rustfmt" ]; })
              pkg-config
              openssl
              cmake
            ];
            shellHook = ''
              export NOSD_ENGINE_DIR="$PWD/../result/lib/nosd/engines"
              echo "NixAIOS devShell — cd packages/nosd && cargo test"
            '';
          };
        });
    };
}
