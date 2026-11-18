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

      perSystem = system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          lib = pkgs.lib;

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

          # The application links libmpv but does not use the optional
          # WebView stack; keep the package closure limited to what it loads.
          appLibs = with pkgs; [
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
            vulkan-headers
            vulkan-loader
            libglvnd
            mesa
            libdrm
            fontconfig
            freetype
            harfbuzz
            mpv-unwrapped
            alsa-lib
          ];

          buildTools = with pkgs; [
            just
            pkg-config
            cmake
            clang
            mold
            rustToolchain

            # Comment pictures copy through the Wayland clipboard
            wl-clipboard

            # Desktop entry and icon installation
            librsvg
            desktop-file-utils
          ];

          profilingTools = with pkgs; [
            perf
            flamegraph
            heaptrack
            hyperfine
          ];

          desktopEntry = ./packaging/linux/rigili.desktop;
          desktopIcon = ./packaging/linux/icons/hicolor/scalable/apps/rigili.svg;
        in
        {
          inherit pkgs lib rustToolchain runtimeLibs appLibs buildTools profilingTools desktopEntry desktopIcon;
        };
    in
    {
      devShells = forAllSystems (system:
        let
          ctx = perSystem system;
          inherit (ctx) pkgs;
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = ctx.buildTools ++ ctx.profilingTools;
            buildInputs = ctx.runtimeLibs;

            RUST_BACKTRACE = "1";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath ctx.runtimeLibs;

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

      packages = forAllSystems (system:
        let
          ctx = perSystem system;
          inherit (ctx) pkgs lib;
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "rigili";
            version = "0.1.0";

            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
                ./packaging
                ./.cargo
              ];
            };

            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "rigili_app" ];
            doCheck = false;

            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              clang
              mold
              librsvg
              makeWrapper
              wayland-protocols
            ];

            buildInputs = ctx.appLibs;

            postInstall = ''
              install -Dm644 ${ctx.desktopEntry} $out/share/applications/rigili.desktop
              install -Dm644 ${ctx.desktopIcon} $out/share/icons/hicolor/scalable/apps/rigili.svg
              for size in 16 24 32 48 64 128 256 512; do
                install -d $out/share/icons/hicolor/''${size}x''${size}/apps
                rsvg-convert -w $size -h $size ${ctx.desktopIcon} \
                  -o $out/share/icons/hicolor/''${size}x''${size}/apps/rigili.png
              done
            '';

            preFixup = ''
              wrapProgram $out/bin/rigili \
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath ctx.appLibs} \
                --prefix PATH : ${lib.makeBinPath [ pkgs.wl-clipboard ]} \
                --run 'if [ -z "''${VK_ICD_FILENAMES:-}" ] && [ -d /run/opengl-driver/share/vulkan/icd.d ]; then export VK_ICD_FILENAMES="$(find /run/opengl-driver/share/vulkan/icd.d -name "*.json" -print | paste -sd: -)"; fi'
            '';

            meta = {
              description = "Native Bilibili desktop client for Linux";
              mainProgram = "rigili";
              platforms = lib.platforms.linux;
            };
          };
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/rigili";
        };
      });
    };
}
