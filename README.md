# Substrate Frontier

Below are some draft (and dirty) implementations that attempt to port "Ethereum 1.0" to Substrate. We list subgoals here and current status.

## Multi-trie Usability

This first aims to be a demo for Substrate multi-trie usability. `substrate-trie` is not generic yet, so here we use Cargo's `[patch]` section to replace `substrate-trie` by a custom implementation.

## Dependency Unification

We have dependencies in many places that either only works for Substrate or Parity Ethereum, while serve similar propose. This makes it not possible to directly pull in dependencies. Below is an incomplete list of crates that have this issue:

* `substrate-primitives` and `ethereum-types`. This is mostly due to `H256` and affects many children crates such as `keccak-hasher` and `rlp`.
* `paritytech/parity-common/trie` and `paritytech/trie`.
