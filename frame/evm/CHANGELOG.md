# Changelog for `pallet-evm`

## Unreleased
- Changed associated type `Currency` to require `fungible::Mutate` and `fungible::Balanced` instead of `Currency`.
- Changed associated type `AccountProvider` to require its `AccountId` to implement `Eq`.
- Added associated type `BlockHashMapping` that requires a `BlockHashMapping` trait implementor. Projects that integrate pallet-ethereum can use this trait to return the ethereum block hash when using `blockhash` Solidity function.