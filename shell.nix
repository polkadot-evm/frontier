let
  mozillaOverlay =
    import (builtins.fetchGit {
      url = "https://github.com/mozilla/nixpkgs-mozilla.git";
      rev = "2292d4b35aa854e312ad2e95c4bb5c293656f21a";
    });
  nixpkgs = import <nixpkgs> { overlays = [ mozillaOverlay ]; };
  rust-stable = with nixpkgs; ((rustChannelOf { date = "2025-07-01"; channel = "nightly"; }).rust.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  });
in
with nixpkgs; pkgs.mkShell {
  nativeBuildInputs = [
    rust-stable
  ];
  buildInputs = [
    clang
    rocksdb
    pkg-config
    openssl.dev
    nodejs
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Security
  ];

  RUST_SRC_PATH = "${rust-stable}/lib/rustlib/src/rust/src";
  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
  PROTOC = "${protobuf}/bin/protoc";
  ROCKSDB_LIB_DIR = "${rocksdb}/lib";
  CFLAGS = "-Wno-error=int-conversion";
  LD_LIBRARY_PATH = "${pkgs.stdenv.cc.cc.lib}/lib";
}
