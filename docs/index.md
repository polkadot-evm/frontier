---
home: true
heroImage: https://v1.vuepress.vuejs.org/hero.png
tagline: Ethereum compatibility layer for Substrate.
actionText: Code →
actionLink: https://github.com/paritytech/frontier
features:
- title: EVM Contract
  details: Frontier allows you to run EVM contracts natively in Substrate, tightly integrated with the rest of the Substrate ecosystem.
- title: RPC Compatibility
  details: All existing Ethereum RPC methods work, so none of your dapps will break.
- title: Substrate Module
  details: Frontier can be easily integrated in your existing Substrate application as a runtime module.
footer: Made by Parity & Wei & PureStake with ❤️
---

Frontier is the suite that provides an Ethereum compatibility layer
for Substrate. It has two components that can be activated separately:

* **Pallet EVM**: This is the pallet that enables functionality of
  running EVM contracts. Existing EVM code can be used from there,
  using addresses and values mapped directly to Substrate.
* **Pallet Ethereum** with **Ethereum compatible RPC methods**: The
  pallet, combined with the RPC module, enables Ethereum block
  emulation, validates Ethereum-encoded transactions, and allows
  existing dapps to be deployed on a Substrate blockchain with minimal
  modifications.
  
Frontier is not a bridge. Frontier enables you to run Ethereum dapps
natively on Substrate, but does not deal with the issues of
communicating with other Ethereum-based networks. If you want a
standalone blockchain, you are looking for Frontier. If you want to
create a parachain that has Ethereum functionality, you need to
combine Frontier with Cumulus. If you do not want EVM functionality
natively but want to communicate with other networks, you need the
Parity Bridge project.

Frontier is still under heavy development. We consider Pallet EVM to
be relatively stable, while some parts of Pallet Ethereum's design
might still be changed. In the future, using the [wrapper
block](https://corepaper.org/substrate/wrapper/) strategy, Frontier
can also eventually function as an Ethereum client, allowing you to
migrate any eth1.x-based blockchains seamlessly over to Substrate,
allowing you to add Substrate-specific features like on-chain
governance and forkless upgrades!
