# Changelog for `fc-rpc`

## Unreleased

* Fix `estimate_gas`: ensure that provided gas limit it never larger than current block's gas limit
* `EthPubSubApi::new` takes an additional `overrides` parameter.
* Fix `estimate_gas` inaccurate issue.
* Use pallet-ethereum 3.0.0-dev.