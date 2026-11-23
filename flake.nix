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

      # Loaded at runtime by winit and wgpu, not linked: X11's for winit's X11 backend, libGL for wgpu's OpenGL
      # backend where Vulkan isn't available.
      runtimeLibraries = with pkgs; [
        wayland
        libxkbcommon
        vulkan-loader
        libGL
        xorg.libX11
        xorg.libXcursor
        xorg.libXrandr
        xorg.libXi
      ];

      # `cargo xtask docs` renders the help file with pandoc's vimdoc writer, new in 3.8.1, which nixpkgs doesn't have
      # yet: pandoc's own static release, as built by pandoc, not from source. Elsewhere nixpkgs', without the writer.
      pandocRelease = {
        x86_64-linux = { arch = "amd64"; hash = "sha256-MlLF/YI0kl9Glz9PF/hSzNpkd3oMYx1XNYYcfrTWkTI="; };
        aarch64-linux = { arch = "arm64"; hash = "sha256-iYtcH2vKuKYCU8yhAn5DR276ItHOmSMruVjNEYA7y5A="; };
      }.${system} or null;
      pandoc = if pandocRelease == null then pkgs.pandoc else pkgs.stdenvNoCC.mkDerivation rec {
        pname = "pandoc";
        version = "3.8.2";
        src = pkgs.fetchurl {
          url = "https://github.com/jgm/pandoc/releases/download/${version}/pandoc-${version}-linux-${pandocRelease.arch}.tar.gz";
          inherit (pandocRelease) hash;
        };
        installPhase = "install -Dm755 bin/pandoc -t $out/bin";
      };
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

        # The launcher entry, for desktops to list Sarthi and open files with it.
        postInstall = ''
          install -Dm644 extra/linux/sarthi.desktop -t $out/share/applications
        '';
      };

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rustToolchain
          wgsl-analyzer
          nil
          # `cargo xtask docs`: renders the help file.
          pandoc
        ];

        env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibraries;

        # Use Vulkan drivers from this flake's mesa instead of the system's.
        env.VK_DRIVER_FILES = "${pkgs.mesa}/share/vulkan/icd.d";
      };
    });
}

