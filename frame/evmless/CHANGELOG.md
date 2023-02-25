# Changelog for `pallet-evm`

## Unreleased
- Added associated type `BlockHashMapping` that requires a `BlockHashMapping` trait implementor. Projects that integrate pallet-ethereum can use this trait to return the ethereum block hash when using `blockhash` Solidity function.