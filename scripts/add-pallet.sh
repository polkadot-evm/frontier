#!/bin/bash
set -e

if [ $# -ne 2 ]; then
  echo "Uso: $0 <pallet-name> <pallet-index>"
  exit 1
fi

PALLET_NAME=$1       # ej: pallet-tkf-cteam
PALLET_INDEX=$2      # ej: 17
PALLET_PATH="frame/$PALLET_NAME"
DEP_IDENT=$(echo "$PALLET_NAME" | tr '-' '_')   # ej: pallet_tkf_cteam

ROOT_CARGO="Cargo.toml"
RUNTIME_CARGO="template/runtime/Cargo.toml"
RUNTIME_LIB="template/runtime/src/lib.rs"

echo ""
echo "🚀 Añadiendo $PALLET_NAME con índice $PALLET_INDEX..."
echo ""

# ==========================
# 1️⃣ ROOT Cargo.toml
# ==========================
if ! grep -q "$PALLET_NAME" "$ROOT_CARGO"; then
  sed -i "/^\[workspace.members\]/a \ \ \"$PALLET_PATH\"," "$ROOT_CARGO"
  echo "📦 Añadido $PALLET_NAME al workspace"
fi

if ! grep -q "$PALLET_NAME" "$ROOT_CARGO"; then
  sed -i "/^\[workspace.dependencies\]/a $PALLET_NAME = { path = \"$PALLET_PATH\", default-features = false }" "$ROOT_CARGO"
  echo "📦 Añadido $PALLET_NAME a las dependencias del workspace"
fi

# ==========================
# 2️⃣ RUNTIME Cargo.toml
# ==========================
if ! grep -q "^$PALLET_NAME" "$RUNTIME_CARGO"; then
  sed -i "/^\[dependencies\]/a $PALLET_NAME = { workspace = true, optional = true }" "$RUNTIME_CARGO"
  echo "📦 Añadido $PALLET_NAME al runtime Cargo.toml"
fi

if ! grep -q "\"$PALLET_NAME/std\"" "$RUNTIME_CARGO"; then
  sed -i "/^std = \[/a \ \ \"$PALLET_NAME/std\"," "$RUNTIME_CARGO"
  echo "⚡ Añadida feature std de $PALLET_NAME al runtime"
fi

# ==========================
# 3️⃣ RUNTIME lib.rs
# ==========================
# Añadir type alias
if ! grep -q "pub type .* = $DEP_IDENT;" "$RUNTIME_LIB"; then
  sed -i "/pub struct Runtime;/a \ \n\t#[runtime::pallet_index($PALLET_INDEX)]\n\tpub type $(echo $PALLET_NAME | sed 's/pallet-tkf-/Tokfin/' | sed 's/-/_/g' | sed -E 's/(^|_)([a-z])/\U\2/g') = $DEP_IDENT;" "$RUNTIME_LIB"
  echo "⚡ Añadido $PALLET_NAME al runtime con índice $PALLET_INDEX"
fi

# Añadir impl Config
if ! grep -q "impl $DEP_IDENT::Config for Runtime" "$RUNTIME_LIB"; then
  echo -e "\nimpl ${DEP_IDENT}::Config for Runtime {}\n" >> "$RUNTIME_LIB"
  echo "⚡ Añadido impl Config para $PALLET_NAME"
fi

echo "✅ Proceso completado."
