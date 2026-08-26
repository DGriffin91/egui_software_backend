{
  description = "development shell for egui_software_backend";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
  flake-utils.lib.eachDefaultSystem (system:
  let
    overlays = [ (import rust-overlay) ];

    pkgs = import nixpkgs {
      inherit system overlays;
    };

    librarys = with pkgs; [
      # required by winit
      libxkbcommon
      wayland
      libx11
    ];
  in
  {
    devShells.default = with pkgs; mkShell {
      buildInputs = [
        rust-bin.stable.latest.default
        rust-analyzer
        pkg-config
        cargo-deny
      ];

      LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath librarys}";
    };
  });
}
