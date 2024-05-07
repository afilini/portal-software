{
  description = "Software for the Portal hardware wallet";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;

          config.android_sdk.accept_license = true;
          config.allowUnfree = true;
        };
        rustVersion = "1.76.0";
        getRust =
          # fullAndroid implies withAndroid
          { fullAndroid ? false, withAndroid ? fullAndroid, withIos ? false, withEmbedded ? false, nightly ? false }:
          let
            rs = if nightly then pkgs.rust-bin.nightly."2024-01-31" else pkgs.rust-bin.stable.${rustVersion};
          in (rs.default.override {
            extensions = [
              "rust-src" # for rust-analyzer
            ];
            targets = []
                        ++ pkgs.lib.optionals withEmbedded ["thumbv7em-none-eabihf"]
                        ++ pkgs.lib.optionals withAndroid ["x86_64-linux-android" "aarch64-linux-android"]
                        ++ pkgs.lib.optionals fullAndroid ["i686-linux-android" "armv7-linux-androideabi"]
                        ++ pkgs.lib.optionals withIos [ "aarch64-apple-ios-sim" "x86_64-apple-ios" "aarch64-apple-ios" ];
          });
        getCrane = data: (crane.mkLib pkgs).overrideToolchain (getRust data);

        # hal is still using miniscript v9.0.1 which doesn't compile on >= 1.71
        halRustPlatform = pkgs.makeRustPlatform {
          cargo = pkgs.rust-bin.stable."1.70.0".minimal;
          rustc = pkgs.rust-bin.stable."1.70.0".minimal;
        };
        rust = getRust { withEmbedded = false; };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rust;
          rustc = rust;
        };

        android = {
          buildToolsVersion = "30.0.3";
          cmakeVersion = "3.6.4111459";
        };
        androidShellHook = ''
          # Add cmake to the path.
          cmake_root="$(echo "$ANDROID_SDK_ROOT/cmake/${android.cmakeVersion}"*/)"
          export PATH="$cmake_root/bin:$PATH"

          # Write out local.properties for Android Studio.
          cat <<EOF > ./sdk/libportal-android/local.properties
          # This file was automatically generated by nix-shell.
          sdk.dir=$ANDROID_SDK_ROOT
          ndk.dir=$ANDROID_NDK_ROOT
          cmake.dir=$cmake_root
          EOF
        '';
        androidComposition = pkgs.androidenv.composeAndroidPackages {
          buildToolsVersions = [ android.buildToolsVersion ];
          platformVersions = [ "33" ];
          includeNDK = true;
          ndkVersion = "23.1.7779620";
          cmakeVersions = [ android.cmakeVersion ];
        };

        defaultDeps = with pkgs; [ cmake SDL2 fltk pango rust-analyzer pkg-config libusb ];
        embeddedDeps = with pkgs; [ probe-rs gcc-arm-embedded qemu gdb openocd clang (getRust { withEmbedded = true; nightly = true; }) ];
        androidDeps = with pkgs; [ cargo-ndk jdk gnupg (getRust { fullAndroid = true; }) ];
        iosDeps = with pkgs; [ (getRust { withIos = true; }) ];
      in
      rec {

        devShells.default = pkgs.mkShell {
          buildInputs = defaultDeps ++ [ rust ];
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
        devShells.embedded = pkgs.mkShell {
          buildInputs = defaultDeps ++ embeddedDeps ++ [ packages.hal ];

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          CC_thumbv7em_none_eabihf = "${pkgs.gcc-arm-embedded}/bin/arm-none-eabi-gcc";
        };
        devShells.android = pkgs.mkShell rec {
          buildInputs = defaultDeps ++ androidDeps;

          ANDROID_SDK_ROOT = "${androidComposition.androidsdk}/libexec/android-sdk";
          ANDROID_NDK_ROOT = "${ANDROID_SDK_ROOT}/ndk-bundle";

          # Ensures that we don't have to use a FHS env by using the nix store's aapt2.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${ANDROID_SDK_ROOT}/build-tools/${android.buildToolsVersion}/aapt2";

          shellHook = androidShellHook;
        };
        devShells.ios = pkgs.mkShell {
          buildInputs = defaultDeps ++ iosDeps;

          shellHook = ''
          ${
            pkgs.lib.optionalString pkgs.stdenv.isDarwin ''export PATH="/usr/bin:$PATH"''
          }
          '';
        };

        packages.hal = halRustPlatform.buildRustPackage rec {
          pname = "hal";
          version = "0.9.3";

          src = pkgs.fetchFromGitHub {
            owner = "stevenroose";
            repo = pname;
            rev = "v${version}";
            hash = "sha256-QOp7YM/R8mhDVbSaABGjRqqqHW288UYWHxezz5dUAwU=";
          };

          cargoHash = "sha256-/+ld3zfyCpylEPUoGoOCBHiski2lle8QW+/zoW/PgmM=";

          meta = with pkgs.lib; {
            description = "the Bitcoin companion";
            homepage = "https://github.com/stevenroose/hal";
            license = licenses.unlicense;
            maintainers = [];
          };
        };

        packages.smallQemu = let
            qemu' = pkgs.qemu.override { hostCpuTargets = ["arm-softmmu"]; };
          in
          pkgs.writeShellScriptBin "qemu-system-arm" ''
            exec ${qemu'}/bin/qemu-system-arm "$@"
          '';

        packages.emulator = pkgs.callPackage ./emulator { inherit pkgs; craneLib = getCrane { withEmbedded = false; }; };
        packages.gui-simulator = pkgs.callPackage ./gui { inherit pkgs; craneLib = getCrane { withEmbedded = false; }; };
        packages.model = pkgs.callPackage ./model { inherit pkgs; craneLib = getCrane { withEmbedded = false; }; };
        packages.sdk = pkgs.callPackage ./sdk { inherit pkgs; craneLib = getCrane { withEmbedded = false; }; };

        packages.firmware-emu = pkgs.callPackage ./firmware rec {
          inherit pkgs;
          rustToolchain = getRust { withEmbedded = true; nightly = true; };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          variant = "emulator";
        };
        packages.firmware = pkgs.callPackage ./firmware rec {
          inherit pkgs;
          rustToolchain = getRust { withEmbedded = true; nightly = true; };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        };

        packages.docker.emulatorImage = pkgs.callPackage ./docker/emulator.nix { inherit pkgs packages; };
        packages.docker.devEnvironment = pkgs.callPackage ./docker/embedded-dev.nix { inherit pkgs packages getRust; };
      }
    );
}
