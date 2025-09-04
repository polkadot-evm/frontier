# pallet-tkf-exchange

📦 **TkfExchange** — Pallet de ejemplo para Tokfinet.

## 📌 Descripción
Este pallet proporciona almacenamiento simple de un valor entero en la blockchain
y puede servir como plantilla para desarrollar nuevos pallets.

## ⚙️ Integración en Runtime

1. Añadir dependencia en `template/runtime/Cargo.toml`:

```toml
pallet-tkf-exchange = { workspace = true, optional = true }
```

2. Activar el feature en la sección `std`:

```toml
"pallet-tkf-exchange/std",
```

3. Registrar en el runtime (`runtime/src/lib.rs`):

```rust
#[runtime::pallet_index(<INDEX>)]
pub type TkfExchange = pallet_tkf_exchange;

impl pallet_tkf_exchange::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
}
```

## 🚀 Uso
- Llamada a `store_something(value: u32)` para guardar un valor en la blockchain.
- Evento emitido: `ValueStored(value, account)`.

---
Generado automáticamente con `scripts/new-pallet.sh`.
