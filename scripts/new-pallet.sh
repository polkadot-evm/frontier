#!/bin/bash
# Uso: ./scripts/new-pallet.sh pallet-nombre
set -e

if [ -z "$1" ]; then
  echo "❌ Uso: $0 <nombre-del-pallet>"
  exit 1
fi

PALLET_NAME=$1
PALLET_DIR="frame/$PALLET_NAME"

# Copiar plantilla
cp -r scripts/helpers/pallet-template "$PALLET_DIR"

# Sustituir el nombre dentro de Cargo.toml y lib.rs
sed -i "s/pallet-template/$PALLET_NAME/g" "$PALLET_DIR/Cargo.toml"
sed -i "s/pallet_template/$PALLET_NAME/g" "$PALLET_DIR/src/lib.rs"

echo "✅ Nuevo pallet creado en $PALLET_DIR"
echo "Ahora añádelo al workspace en el Cargo.toml del root y runtime."

# Crear README.md
cat <<EOF > "$PALLET_DIR/README.md"
# pallet-$NAME

Este pallet forma parte del runtime **Tokfin**.  
Documenta aquí su funcionalidad.
EOF