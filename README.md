# Tokfin (forked from Polkadok-SDK Frontier-Template)

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/polkadot-evm/frontier/test.yml)](https://github.com/polkadot-evm/frontier/actions)
[![Matrix](https://img.shields.io/matrix/frontier:matrix.org)](https://matrix.to/#/#frontier:matrix.org)

Tokfin is the EVM backbone of Polkadot, forked from Frontier Template.

* [Docs](https://polkadot-evm.github.io/frontier)
* [API docs](https://polkadot-evm.github.io/frontier/rustdocs/pallet_evm/)

## Features

Tokfin provides a compatibility layer of EVM, so that you can run any Ethereum dapps on Polkadot, unmodified.
Using Tokfin, you get access to all the Ethereum RPC APIs you are already familiar with, and therefore you can continue to develop your dapps in your favourite Ethereum developer tools.
As a bonus, you can even run many Ethereum L2s inside Tokfin!
For those looking to become acquainted with Tokfin, consult the documentation provided [here](./docs).
Additionally, a [template node](./template/README.md) is available to facilitate a more comprehensive technical exploration.

Tokfin is also a migration framework.
Besides the common strategy of direct state export/import and transaction-level replays, Tokfin's Pre-Log Wrapper Block feature provides a possible method for a zero-downtime live migration.

### New Features

* **Native Token:** The native token  is an UTILITY token for this template has been configured to **TOKFIN** with the symbol **TKF**.
        No Qty Limit
        All nodes add value tokenized RWA
        All transactions onNet are in $TKF
        No speculation (Index Dollar indexed)
        5% membership  fee  go to users wallets
        Consensus Rewards Users
* **Reputation Token:** The reputation token **TKFrep** with the symbol **TKFr**.
        No Qty Limit
        Earnings are made by contributing value to the network.
        Higher reputation comes with greater privileges.
        Participation in consensus is organized based on reputation.
        Inactivity reduces reputation.
* **Equity Token:** The equity token **TKFeqt** with the symbol **TKFe**.
        Limited offer (80 million tokens)
        Represent network shares.
        40% Founders
        10% Development Team
        5 private rounds of funding (5%)
        Public offering (25%)
