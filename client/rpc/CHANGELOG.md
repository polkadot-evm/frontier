# Changelog for `fc-rpc`

## Unreleased

* Return a JSON-RPC error (EIP-1898 `-32001` "Resource not found") when an explicit block hash given as a block parameter cannot be resolved, instead of silently returning a zero/default/empty result.
* Fix `estimate_gas`: ensure that provided gas limit it never larger than current block's gas limit
* `EthPubSubApi::new` takes an additional `overrides` parameter.
* Fix `estimate_gas` inaccurate issue.
* Use pallet-ethereum 3.0.0-dev.
* `EthFilterApi::new` takes an additional `backend` parameter.
* Bump `fp-storage` to `2.0.0-dev`.
* Bump `fc-db` to `2.0.0-dev`.
* Removed on-memory pending transactions in favor of transaction pool.