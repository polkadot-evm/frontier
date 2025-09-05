# Polkadot Frontier

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/polkadot-evm/frontier/test.yml)](https://github.com/polkadot-evm/frontier/actions)
[![Matrix](https://img.shields.io/matrix/frontier:matrix.org)](https://matrix.to/#/#frontier:matrix.org)

Frontier is the EVM backbone of Polkadot.

* [Docs](https://polkadot-evm.github.io/frontier)
* [API docs](https://polkadot-evm.github.io/frontier/rustdocs/pallet_evm/)

## Features

Frontier provides a compatibility layer of EVM, so that you can run any Ethereum dapps on Polkadot, unmodified.
Using Frontier, you get access to all the Ethereum RPC APIs you are already familiar with, and therefore you can continue to develop your dapps in your favourite Ethereum developer tools.
As a bonus, you can even run many Ethereum L2s inside Frontier!
For those looking to become acquainted with Frontier, consult the documentation provided [here](./docs).
Additionally, a [template node](./template/README.md) is available to facilitate a more comprehensive technical exploration.

Frontier is also a migration framework.
Besides the common strategy of direct state export/import and transaction-level replays, Frontier's Pre-Log Wrapper Block feature provides a possible method for a zero-downtime live migration.
