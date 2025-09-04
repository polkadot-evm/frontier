#!/bin/bash
set -e

if [ -z "$1" ]; then
  echo "❌ Uso: $0 <nombre-del-pallet>"
  exit 1
fi

PALLET_NAME=$1
PALLET_DIR="frame/$PALLET_NAME"

if [ -d "$PALLET_DIR" ]; then
  echo "❌ El pallet $PALLET_NAME ya existe"
  exit 1
fi

# Crear estructura básica
mkdir -p $PALLET_DIR/src
cat > $PALLET_DIR/Cargo.toml <<EOF
[package]
name = "$PALLET_NAME"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"

[dependencies]
frame-support = { workspace = true, default-features = false }
frame-system  = { workspace = true, default-features = false }
scale-codec = { workspace = true, features = ["derive"], default-features = false }
scale-info = { workspace = true, features = ["derive"], default-features = false }
sp-std = { workspace = true, default-features = false }
log = { workspace = true }

[features]
default = ["std"]
std = [
    "frame-support/std",
    "frame-system/std",
    "scale-codec/std",
    "scale-info/std",
    "sp-std/std",
    "log/std",
]
EOF

# Pallet lib.rs con imports correctos
cat > $PALLET_DIR/src/lib.rs <<EOF
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{pallet_prelude::*, dispatch::DispatchResult};
    use frame_system::{pallet_prelude::*, ensure_signed};

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::storage]
    pub(super) type StoredValue<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn store_something(origin: OriginFor<T>, value: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            StoredValue::<T>::put(value);
            log::info!("👉 {:?} stored value: {}", who, value);
            Ok(())
        }
    }
}
EOF

# README para documentar el pallet
cat > $PALLET_DIR/README.md <<EOF
# $PALLET_NAME

Esqueleto de pallet FRAME para Tokfinet.

## Funcionalidad
- Ejemplo básico de almacenamiento (`StoredValue`)
- Ejemplo de extrinsic (`store_something`)

## Uso
1. Añadir al workspace en el Cargo.toml raíz
2. Añadir al runtime con \`scripts/add-pallet.sh $PALLET_NAME <índice>\`
EOF

echo "✅ Pallet $PALLET_NAME creado en $PALLET_DIR"
