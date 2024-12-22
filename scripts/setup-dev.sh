#!/usr/bin/env bash

function install_rustup {
  echo "Installing Rust toolchain..."
  if rustup --version &> /dev/null; then
    echo "Rust toolchain is already installed"
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source "$HOME"/.cargo/env
  fi
  rustup show
}

function install_cargo_binary {
  if [ -z "$1" ]; then
    echo "Error: Crate name is required."
    return 1
  fi

  CRATE_NAME=$1
  BIN_NAME=${2:-$1}

  if command -v "$BIN_NAME" &> /dev/null; then
    echo "$CRATE_NAME is already installed"
  else
    cargo install "$CRATE_NAME" --locked
  fi
}

install_rustup
install_cargo_binary "taplo-cli" "taplo"
