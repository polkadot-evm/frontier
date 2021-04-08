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

* `fp-consensus`: Consensus layer primitives.
  ![Crates.io](https://img.shields.io/crates/v/fp-consensus)
* `fp-evm`: EVM primitives. ![Crates.io](https://img.shields.io/crates/v/fp-evm)
* `fp-rpc`: RPC primitives. ![Crates.io](https://img.shields.io/crates/v/fp-rpc)
* `fp-storage`: Well-known storage information.
  ![Crates.io](https://img.shields.io/crates/v/fp-storage)

### Pallets

Those pallets serve as runtime components for projects using Frontier.

* `pallet-evm`: EVM execution handling.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm)
* `pallet-ethereum`: Ethereum block handling.
  ![Crates.io](https://img.shields.io/crates/v/pallet-ethereum)
* `pallet-dynamic-fee`: Extends the fee handling logic so that it can be changed
  within the runtime.
  ![Crates.io](https://img.shields.io/crates/v/pallet-dynamic-fee)

### EVM Pallet precompiles

Those precompiles can be used together with `pallet-evm` for additional
functionalities of the EVM executor.

* `pallet-evm-precompile-simple`: Four basic precompiles in Ethereum EVMs.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-simple)
* `pallet-evm-precompile-blake2`: BLAKE2 precompile.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-blake2)
* `pallet-evm-precompile-bn128`: BN128 precompile.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-bn128)
* `pallet-evm-precompile-ed25519`: ED25519 precompile.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-ed25519)
* `pallet-evm-precompile-modexp`: MODEXP precompile.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-modexp)
* `pallet-evm-precompile-sha3fips`: Standard SHA3 precompile.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-sha3fips)
* `pallet-evm-precompile-dispatch`: Enable interoperability between EVM
  contracts and other Substrate runtime components.
  ![Crates.io](https://img.shields.io/crates/v/pallet-evm-precompile-dispatch)

### Client-side libraries

Those are libraries that should be used on client-side to enable RPC, block hash
mapping, and other features.

* `fc-consensus`: Consensus block import.
  ![Crates.io](https://img.shields.io/crates/v/fc-consensus)
* `fc-db`: Frontier-specific database backend.
  ![Crates.io](https://img.shields.io/crates/v/fc-db)
* `fc-mapping-sync`: Block hash mapping syncing logic.
  ![Crates.io](https://img.shields.io/crates/v/fc-mapping-sync)
* `fc-rpc-core`: Core RPC logic.
  ![Crates.io](https://img.shields.io/crates/v/fc-rpc-core)
* `fc-rpc`: RPC implementation.
  ![Crates.io](https://img.shields.io/crates/v/fc-rpc)