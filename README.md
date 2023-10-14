# Polkadot Frontier

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/paritytech/frontier/rust.yml)](https://github.com/paritytech/frontier/actions)
[![Matrix](https://img.shields.io/matrix/frontier:matrix.org)](https://matrix.to/#/#frontier:matrix.org)

Frontier is the EVM backbone of Polkadot.

## Features

Frontier provides a compatibility layer of EVM, so that you can run any Ethereum
dapps on Polkadot, unmodified. Using Frontier, you get access to all of the
Ethereum RPC APIs you are already familiar with, and therefore you can continue
to develop your dapps in your favourite Ethereum developer tools. As a bonus,
you can even run many Ethereum L2s inside Frontier!

Frontier is also a migration framework. Besides the common strategy of direct
state export/import and transaction-level replays, Frontier's Pre-Log Wrapper
Block feature provides a possible method for a zero-downtime live migration.

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

## Development workflow

### Pull request

All changes (except new releases) are handled through pull requests.

### Versioning

Frontier follows [Semantic Versioning](https://semver.org/). An unreleased crate
in the repository will have the `-dev` suffix in the end, and we do rolling
releases.

When you make a pull request against this repository, please also update the
affected crates' versions, using the following rules. Note that the rules should
be applied recursively -- if a change modifies any upper crate's dependency
(even just the `Cargo.toml` file), then the upper crate will also need to apply
those rules.

Additionally, if your change is notable, then you should also modify the
corresponding `CHANGELOG.md` file, in the "Unreleased" section.

If the affected crate already has `-dev` suffix:

* If your change is a patch, then you do not have to update any versions.
* If your change introduces a new feature, please check if the local version
  already had its minor version bumped, if not, bump it.
* If your change modifies the current interface, please check if the local
  version already had its major version bumped, if not, bump it.

If the affected crate does not yet have `-dev` suffix:

* If your change is a patch, then bump the patch version, and add `-dev` suffix.
* If your change introduces a new feature, then bump the minor version, and add
  `-dev` suffix.
* If your change modifies the current interface, then bump the major version,
  and add `-dev` suffix.

If your pull request introduces a new crate, please set its version to
`1.0.0-dev`.