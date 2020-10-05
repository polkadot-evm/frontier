# Frontier

![GitHub Workflow Status](https://img.shields.io/github/workflow/status/paritytech/frontier/Rust)
![Matrix](https://img.shields.io/matrix/frontier:matrix.org)

Frontier is Substrate's Ethereum compatibility layer. It allows you to run
unmodified Ethereum dapps.

The goal of Ethereum compatibility layer is to be able to:

* Run a normal web3 application via the compatibility layer, using local nodes,
  where an extra bridge binary is acceptable.
* Be able to import state from Ethereum mainnet.

It consists of the following components:

* **[pallet-evm](https://github.com/paritytech/substrate/tree/master/frame/evm)**:
  EVM execution engine for Substrate.
* **pallet-ethereum**: Emulation of full Ethereum block processing.
* **rpc-ethereum**: Compatibility layer for web3 RPC methods.

## Development notes

Frontier is still work-in-progress. Below are some notes about the development.

### Runtime

A few notes on the EthereumRuntimeApi's runtime implementation requirements:

- For supporting author rpc call, the FindAuthor trait must be implemented in an
arbitrary struct. This implementation must call the authorities accessor in either
Aura or Babe and convert the authority id response to H160 using
pallet_evm::HashTruncateConvertAccountId::convert_account_id.

The struct implementing FindAuthor is passed as the FindAuthor associated type's 
value for pallet_ethereum.

An Aura example for this is available in the template's runtime (EthereumFindAuthor).

- For supporting chain_id rpc call, a u64 ChainId constant must be defined.

- For supporting gas_price rpc call, FeeCalculator trait must be implemented in an
arbitrary struct. An example FixedGasPrice is available in the template's runtime.

### Vendor folder

The vendor folder contains dependencies that contains changes that has not yet
been upstreamed. Once the upstreaming process is finished, the corresponding
submodule should be removed from vendor folder, and directly use upstream.

To install those submodules, from the frontier root folder:

```sh
git submodule init
git submodule update
```

### Use local version of Substrate

1. Override your local cargo config to point to your local substrate (pointing to your WIP branch): place `paths = ["path/to/substrate"]` in `~/.cargo/config`.
2. You are good to go.

Remember to comment out the override after it is done to avoid mysterious build issues on other repo.
