# Changelog for `pallet-ethereum`

## Unreleased

* Uses unreleased pallet-evm 5.0.0-dev
* Fix `Event::Executed` for transaction `Call`
* Changed `transact` dispatchable signature. Now an H160 `source` matching the signed transaction must be provided. A mismatch between the two will result on the `validate_unsigned` logic rejecting the transaction.
* `recover_signer` is now removed from the transaction execution logic and only kept in the `validate_unsigned` function as the only source of truth for the signer.