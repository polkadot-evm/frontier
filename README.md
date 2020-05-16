# Frontier

Frontier is Substrate's Ethereum compatibility layer. It allows you to
run unmodified Ethereum dapps.

The goal of Ethereum compatibility layer is to be able to:

* Run a normal web3 application via the compatibility layer, using
  local nodes, where an extra bridge binary is acceptable.
* Be able to import state from Ethereum mainnet.

It consists of the following components:

* **[pallet-evm](https://github.com/paritytech/substrate/tree/master/frame/evm)**:
  EVM execution engine for Substrate.
* **pallet-ethereum**: Emulation of full Ethereum block processing.
* **rpc-ethereum**: Compatibility layer for web3 RPC methods.

## Development notes

Frontier is still work-in-progress. Below are some notes about the development.

## Vendor folder

The vendor folder contains dependencies that contains changes that has not yet
been upstreamed. Once the upstreaming process is finished, the corresponding
submodule should be removed from vendor folder, and directly use upstream.

The `substrate` submodule contains a large quantity of dependencies, so they
should directly use `path` directive in dependency declarations. For other
dependencies, they should use Cargo's patch feature in workspace declaration.
