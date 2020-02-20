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
