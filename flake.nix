{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, flake-utils, fenix, ... }: flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs { inherit system; };
      fenixLib = fenix.packages.${system};
      rustToolchain = fenixLib.stable.toolchain;
      rustPlatform = pkgs.makeRustPlatform { cargo = rustToolchain; rustc = rustToolchain; };

      # Loaded at runtime by winit and wgpu, not linked
      runtimeLibraries = with pkgs; [
        wayland
        libxkbcommon
        vulkan-loader
      ];
    in
    {
      packages.default = rustPlatform.buildRustPackage {
        pname = "sarthi";
        version = (pkgs.lib.importTOML ./Cargo.toml).workspace.package.version;
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;

        # The tests need a GPU, fonts and Neovim, which the build sandbox doesn't have.
        doCheck = false;

        postFixup = ''
          patchelf --add-rpath ${pkgs.lib.makeLibraryPath runtimeLibraries} $out/bin/sarthi
        '';
      };

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rustToolchain
          wgsl-analyzer
          nil
        ];

        env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibraries;

        # Use Vulkan drivers from this flake's mesa instead of the system's.
        env.VK_DRIVER_FILES = "${pkgs.mesa}/share/vulkan/icd.d";
      };
    });
}

