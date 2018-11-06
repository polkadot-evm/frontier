with import <nixpkgs> { };

stdenv.mkDerivation {
  name = "shasper-env";
  buildInputs = [
    rustup
    gcc
    pkgconfig
    libudev
    openssl
	cmake
  ];

  shellHook = ''
    export PATH=~/.cargo/bin:$PATH
	export LD_LIBRARY_PATH=${stdenv.cc.cc.lib}/lib
  '';
}
