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
    in
    {
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rustToolchain
          wgsl-analyzer
          nil
        ];

        env.LD_LIBRARY_PATH = with pkgs; lib.makeLibraryPath [
          wayland
          libxkbcommon
          vulkan-loader
        ];

        # Use Vulkan drivers from this flake's mesa instead of the system's.
        env.VK_DRIVER_FILES = "${pkgs.mesa}/share/vulkan/icd.d";
      };
    });
}

