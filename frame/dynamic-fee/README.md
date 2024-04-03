# Dynamic fee pallet

The dynamic fee pallet allows a Substrate blockchain with Frontier to simulate the functionality of Ethereum dynamic fee adjustment.

## Overview

The pallet works by keeping track of the current minimum gas price in a Substrate storage `MinGasPrice`.
Each Substrate block, the proposer can submit an inherent extrinsic `note_min_gas_price_target`.
When a block is built, the minimum gas price is adjusted similar to Ethereum's algorithm.

## Usage

To use the dynamic fee pallet, first include the pallet in runtime by implementing `pallet_dynamic_fee::Config`.
The `MinGasPriceBoundDivisor` is a divisor used to set how much the minimum gas price is adjusted each block.
You can set it to `1024` to get the same algorithm as Ethereum. After implementing `pallet_dynamic_fee::Config`, include the pallet in the runtime definition.

With the pallet in place, you can now extend the node to allow it to vote on the minimum gas price target, via the inherent data providers.
Locate in the node service code where the `InherentDataProviders` struct is built, and add `pallet_dynamic_fee`'s inherent data provider.
The inherent data provider requires a target gas price parameter to be provided. For that, you simply need to add a new custom command line argument.

An example code snippet is shown below:

```rust
fn inherent_data_providers(
    target_min_gas_price: U256
) -> Result<sp_inherents::InherentDataProviders, ServiceError> {
    let inherent_data_providers = sp_inherents::InherentDataProviders::new();

    if !inherent_data_providers.has_provider(&pallet_dynamic_fee::INHERENT_IDENTIFIER) {
        inherent_data_providers
            .register_provider(pallet_dynamic_fee::InherentDataProvider(target_min_gas_price))
            .map_err(Into::into)
            .map_err(sp_consensus::Error::InherentData)?;
    }

    Ok(inherent_data_providers)
}
```

License: Apache-2.0
