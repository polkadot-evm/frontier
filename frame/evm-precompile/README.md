# EVM Precompile Module

The EVM Precompile Module allows using Precompiles with arbitrary addresses, potentially more than one.

A `StorageMap` keeps track of the Precompiles on-chain, where:
- key: `H160`
- value: `PrecompileLabel`

A `PrecompileLabel` determines which functionality the Precompile has. It is declared as a `BoundedVec<u8, ConstU32<32>>`, which means the user is free to choose a label (e.g.: `b"Sha3FIPS512"`) that's up-to 32 bytes long.

`OnChainPrecompiles` implements the `PrecompileSet` trait, where the Precompile addresses are routed to the appropriate `Precompile::execute` implementation according to the on-chan mapping.

