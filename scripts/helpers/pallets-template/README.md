**caja de herramientas interna del proyecto**, 

con **scripts, plantillas y documentación**, de forma que cualquiera (tú o tu equipo) pueda generar pallets y mantener el runtime sin tener que repetir todo a mano.

## 📂 Carpeta `scripts/`

Crea una carpeta en el root del proyecto:

```
tokfinnet/
 ├─ frame/
 ├─ template/
 ├─ scripts/
 │   ├─ new-pallet.sh
 │   ├─ add-pallet.sh
 │   └─ helpers/
 │       └─ pallet-template/
 │           ├─ Cargo.toml
 │           └─ src/lib.rs
```


## 🛠️ 1. Script para crear un nuevo pallet (`new-pallet.sh`)

## 🛠️ 2. Script para añadir el pallet al runtime (`add-pallet.sh`)

## 🛠️ 3. Plantilla de pallet (`scripts/helpers/pallet-template/`)

### Cargo.toml

### src/lib.rs

## 🚀 Flujo de trabajo

1. Crear un pallet nuevo:

```bash
./scripts/new-pallet.sh pallet-tkf-storage
```

2. Añadirlo al runtime con un índice libre:

```bash
./scripts/add-pallet.sh tkf-storage 14  (14 es un numero de ejemplo ver el index que toca 0,1,2....N)
```

3. Compilar:

SKIP_WASM_BUILD=1 cargo build --release

