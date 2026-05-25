{
  description = "Development shell with CUDA, cuDNN, Python, and Vulkan support";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nixGL.url = "github:guibou/nixGL/refs/pull/218/merge";
    nixGL.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      nixGL,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        lib = pkgs.lib;
        buildInputs = [
          pkgs.nixfmt
          pkgs.udev
          pkgs.alsa-lib
          pkgs.vulkan-loader
          pkgs.vulkan-validation-layers
          pkgs.vulkan-tools
          pkgs.xorg.libX11
          pkgs.xorg.libXcursor
          pkgs.xorg.libXi
          pkgs.xorg.libXrandr # x11 feature
          pkgs.libxkbcommon
          pkgs.wayland
          pkgs.mesa
          pkgs.renderdoc # wayland feature
          pkgs.shaderc
          pkgs.libGL
          # pkgs.libglvnd
          pkgs.pkg-config
          pkgs.cargo
          pkgs.rustc
          pkgs.rust-analyzer
          pkgs.clippy
          pkgs.wgsl-analyzer
        ];
      in
      {
        # Thanks to https://github.com/pbsds for the help with getting cuda to work on a non NixOS machine with the code below!
        devShells = rec {
          default = pkgs.mkShell {
            packages = buildInputs ++ [
            ];

            LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;

          };
          nixgl-nvidia = default.overrideAttrs (old: {
            packages = (old.packages or [ ]) ++ [
              nixGL.packages.${system}.nixGLNvidia
            ];
            shellHook = ''
                  ${old.shellHook or ""}
                                source <( grep '^ *export ' ${
                                  nixGL.packages.${system}.nixGLNvidia
                                }/bin/nixGLNvidia-* )
              export CUDA_PATH=${pkgs.cudatoolkit}
            '';
          });
          nixgl-intel = default.overrideAttrs (old: {
            packages = (old.packages or [ ]) ++ [
              nixGL.packages.${system}.nixGLNvidia
            ];
            shellHook = ''
              ${old.shellHook or ""}
              source <( grep '^ *export ' ${nixGL.packages.${system}.nixGLIntel}/bin/nixGLIntel )
            '';
          });
        };

      }
    );
}
