# Changelog for `fc-rpc`

## Unreleased

* Fix log filters ignoring `fromBlock`: the first `eth_getFilterChanges` poll now scans the requested range instead of reporting only what the logs journal received after the filter was created.
* Fix `estimate_gas`: ensure that provided gas limit it never larger than current block's gas limit
* `EthPubSubApi::new` takes an additional `overrides` parameter.
* Fix `estimate_gas` inaccurate issue.
* Use pallet-ethereum 3.0.0-dev.
* `EthFilterApi::new` takes an additional `backend` parameter.
* Bump `fp-storage` to `2.0.0-dev`.
* Bump `fc-db` to `2.0.0-dev`.
* Removed on-memory pending transactions in favor of transaction pool.