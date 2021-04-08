# Frontier

![GitHub Workflow Status](https://img.shields.io/github/workflow/status/paritytech/frontier/Rust)
![Matrix](https://img.shields.io/matrix/frontier:matrix.org)

Frontier is Substrate's Ethereum compatibility layer. It allows you to run
unmodified Ethereum dapps.

The goal of Ethereum compatibility layer is to be able to:

* Run a normal web3 application via the compatibility layer, using local nodes,
  where an extra bridge binary is acceptable.
* Be able to import state from Ethereum mainnet.

## Releases

### Primitives

Those are suitable to be included in a runtime. Primitives are structures shared
by higher-level code.

* `fp-consensus` (![Crates.io](https://img.shields.io/crates/v/fp-consensus)):
  Consensus layer primitives.
* `fp-evm` (![Crates.io](https://img.shields.io/crates/v/fp-evm)): EVM
  primitives.
* `fp-rpc` (![Crates.io](https://img.shields.io/crates/v/fp-rpc)): RPC
  primitives.
* `fp-storage` (![Crates.io](https://img.shields.io/crates/v/fp-storage)):
  Well-known storage information.

### Pallets

Those pallets serve as runtime components for projects using Frontier.

* `pallet-evm` (![Crates.io](https://img.shields.io/crates/v/pallet-evm)): EVM
  execution handling.
* `pallet-ethereum`
  (![Crates.io](https://img.shields.io/crates/v/pallet-ethereum)): Ethereum
  block handling.
* `pallet-dynamic-fee`
  (![Crates.io](https://img.shields.io/crates/v/pallet-dynamic-fee)): Extends
  the fee handling logic so that it can be changed within the runtime.

### EVM Pallet precompiles

Those precompiles can be used together with `pallet-evm` for additional
functionalities of the EVM executor.

* `pallet-evm-precompile-simple`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-simple)):
  Four basic precompiles in Ethereum EVMs.
* `pallet-evm-precompile-blake2`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-blake2)):
  BLAKE2 precompile.
* `pallet-evm-precompile-bn128`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-bn128)):
  BN128 precompile.
* `pallet-evm-precompile-ed25519`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-ed25519)):
  ED25519 precompile.
* `pallet-evm-precompile-modexp`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-modexp)):
  MODEXP precompile.
* `pallet-evm-precompile-sha3fips`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-sha3fips)):
  Standard SHA3 precompile.
* `pallet-evm-precompile-dispatch`
  (![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-dispatch)):
  Enable interoperability between EVM contracts and other Substrate runtime
  components.

### Client-side libraries

Those are libraries that should be used on client-side to enable RPC, block hash
mapping, and other features.

* `fc-consensus` (![Crates.io](https://img.shields.io/crates/v/fc-consensus)):
  Consensus block import.
* `fc-db` (![Crates.io](https://img.shields.io/crates/v/fc-db)):
  Frontier-specific database backend.
* `fc-mapping-sync`
  (![Crates.io](https://img.shields.io/crates/v/fc-mapping-sync)): Block hash
  mapping syncing logic.
* `fc-rpc-core` (![Crates.io](https://img.shields.io/crates/v/fc-rpc-core)):
  Core RPC logic.
* `fc-rpc` (![Crates.io](https://img.shields.io/crates/v/fc-rpc)): RPC implementation.