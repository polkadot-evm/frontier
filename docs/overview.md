# Overview

Frontier is the suite that provides an Ethereum compatibility layer for Substrate. It has two components that can be activated separately:

- Pallet EVM: This is the pallet that enables functionality of running EVM contracts. Existing EVM code can be used from there, using addresses and values mapped directly to Substrate.
- Pallet Ethereum with Ethereum compatible RPC methods: The pallet, combined with the RPC module, enables Ethereum block emulation, validates Ethereum-encoded transactions, and allows existing dapps to be deployed on a Substrate blockchain with minimal modifications.

## EVM execution only

In many situations, a Substrate blockchain may only want to include EVM execution capatibilities. In this way, it functions similarly to `pallet-contracts`, integrates with Substrate better and is less intrusive. The module, and its EVM execution capatibilties, can be added or removed at any moment via forkless upgrades. With EVM execution only, Substrate uses its account model fully and signs transactions on behalf of EVM accounts.

In this model, however, Ethereum RPCs are not available, and dapps must rewrite their frontend using the Substrate API. If this is the intended way of usage, take a look at the [`pallet-evm`](./pallet-evm.md) documentation.

## Post-block generation

On other situations, a full emulation of Ethereum may be desired so that Ethereum RPCs become available. In this model, a full Ethereum block is emulated within the Substrate runtime, and is generated post-block for the consumption rest of the APIs. In addition to Substrate account signing, traditional Ethereum transactions are also processed and validated.

If this is the intended way of usage, take a look at the [`pallet-ethereum`](./pallet-ethereum.md) documentation.

## Pre-block feeding

An Ethereum-based blockchain can use the pre-block feeding strategy to migrate to Substrate. In the post-block generation model, the Ethereum block is generated *after* runtime execution. In the pre-block feeding model, the Ethereum block is feeded in *before* runtime execution.

A blockchain can first use pre-block feeding with empty extrinsic requirement. In this way, because no other external information is feeded, combined with a suitable consensus engine, one Ethereum block
will have an exact corresponding Substrate block. This is called the [wrapper block](https://corepaper.org/substrate/wrapper/) strategy, and it allows Frontier to function as a normal Ethereum client.

With a sufficient number of the network running a Frontier node, the blockchain can then initiate a hard fork, allowing extrinsic to be added in. From there on, the blockchain is migrated to Substrate and can enjoy Substrate-specific features like on-chain governance and forkless upgrade.

A complete in-storage pre-block feeding requires using Substrate's child storage. It can also be implemented using the stateless client strategy to eliminate that need.

Pre-block feeding is still work-in-progress.