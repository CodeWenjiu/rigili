{
  description = "Rigili - a native Bilibili desktop client for Linux, built with Rust and GPUI Kit";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" ];
          };

          # Libraries linked or dlopen()ed by GPUI, winit, blade, libmpv and wry.
          runtimeLibs = with pkgs; [
            # Windowing and input
            wayland
            wayland-protocols
            libxkbcommon
            libx11
            libxcb
            libxcb-util
            libxcb-wm
            libxcursor
            libxrandr
            libxi
            libxinerama
            libxext
            libxrender

            # GPUI renders through Vulkan on Linux
            vulkan-headers
            vulkan-loader
            vulkan-validation-layers
            libglvnd
            mesa
            libdrm

            # Fonts and text shaping
            fontconfig
            freetype
            harfbuzz

            # Video playback (planned: libmpv) and audio
            mpv-unwrapped
            alsa-lib

            # Networking and compression
            openssl
            zstd

            # WebView, for login and optional embedded pages
            webkitgtk_4_1
            libsoup_3
            gtk3
            glib
          ];

          buildTools = with pkgs; [
            just
            pkg-config
            cmake
            clang
            mold
            rustToolchain
          ];
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = buildTools;
            buildInputs = runtimeLibs;

            RUST_BACKTRACE = "1";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;

            shellHook = ''
              # On NixOS the Vulkan ICDs are registered under /run/opengl-driver.
              # VK_ICD_FILENAMES is a colon-separated list of JSON files.
              if [ -d /run/opengl-driver/share/vulkan/icd.d ]; then
                export VK_ICD_FILENAMES="$(find /run/opengl-driver/share/vulkan/icd.d -name '*.json' -print | paste -sd: -)"
              fi

              echo "Rigili dev shell - $(rustc --version)"
            '';
          };
        });
    };
}