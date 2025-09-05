# Tokfin Architecture

## Pallets Base
- **pallet-tkf-master** → control principal de sistema.
- **pallet-tkf-exchange** → lógica de intercambio, más allá de un simple DEX.
- **pallet-tkf-foundation** → gestión de cuentas fundacionales.
- **pallet-tkf-storage** → almacenamiento on-chain extendido.

## Orchestrator
El **pallet-orchestrator** coordina:
- Inicialización del genesis.
- Creación de cuentas de sistema.
- Sincronización entre pallets.
- Escalado progresivo.

## Road to N*N TPS
- Sharding y paralelización inspirados en Polkadot.
- Optimización de weights y benchmarking continuo.
