# Accounts

Tokfin provides two different strategies for handling `H160` addresses.

## H256 -> H160 mapping

The first strategy consists of a truncated hash scheme, where the first 160 LE bytes of a `H256` address are used to form the `H160` address.

`AccountId32` is the Account type used for `frame_system::pallet::Config::AccountId`.

The Runtime's `Signature` type is configured as [`sp_runtime::MultiSignature`](https://docs.rs/sp-runtime/2.0.1/sp_runtime/enum.MultiSignature.html), which means signatures can be:

- `Sr25519`
- `Ed25519`
- `ECDSA`

## Native H160 via AccountId20

The second strategy consists of using `fp-account` so that `AccountId20` is the Account type used for `frame_system::pallet::Config::AccountId`.

The Runtime's `Signature` type is configured as `EthereumSigner`, which means only `ECDSA` signatures are supported.
