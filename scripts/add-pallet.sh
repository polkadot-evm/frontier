#!/bin/bash
# Uso: ./scripts/add-pallet.sh pallet-nombre índice
set -e

if [ -z "$1" ] || [ -z "$2" ]; then
  echo "❌ Uso: $0 <nombre-del-pallet> <índice>"
  exit 1
fi

PALLET_NAME=$1
INDEX=$2
RUNTIME_DIR="template/runtime"

# 1. Añadir dependencia al runtime/Cargo.toml
grep -q "$PALLET_NAME" $RUNTIME_DIR/Cargo.toml || \
echo "pallet-$PALLET_NAME = { workspace = true }" >> $RUNTIME_DIR/Cargo.toml

# 2. Añadir a features std
sed -i "/std = \[/a \ \ \"pallet-$PALLET_NAME/std\"," $RUNTIME_DIR/Cargo.toml

# 3. Añadir al runtime/lib.rs
cat <<EOF >> $RUNTIME_DIR/src/lib.rs

#[runtime::pallet_index($INDEX)]
pub type ${PALLET_NAME^} = pallet_$PALLET_NAME;

impl pallet_$PALLET_NAME::Config for Runtime {}
EOF

echo "✅ Pallet $PALLET_NAME añadido al runtime con índice $INDEX"
