Frontier provides two different strategies for handling `H160` addresses.

# `H256` -> `H160` mapping (`AccountId32`)

The first strategy consists of of a truncated hash scheme, where the first 160 LE bytes of a `H256` address are used to form the `H160` address.

`AccountId32` is the Account type used for `frame_system::pallet::Config::AccountId`.

The Runtime's `Signature` type is configured as [`sp_runtime::MultiSignature`](https://docs.rs/sp-runtime/2.0.1/sp_runtime/enum.MultiSignature.html), which means signatures can be:
- `Sr25519`
- `Ed25519`
- `ECDSA`

# Native `H160` (`AccountId20`)

The second strategy consists of using `fp-account` so that `AccountId20` is the Account type used for `frame_system::pallet::Config::AccountId`.

The Runtime's `Signature` type is configured as `EthereumSigner`, which means only `ECDSA` signatures are supported.

Frontier's Template will pre-fund several well-known addresses that (mostly) contain the letters "th" in their names to remind you that they are for ethereum-compatible usage. These addresses are derived from Substrate's canonical mnemonic: `bottom drive obey lake curtain smoke basket hold race lonely fit walk`
```
# Alith:
- Address: 0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac
- PrivKey: 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133

# Baltathar:
- Address: 0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0
- PrivKey: 0x8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b

# Charleth:
- Address: 0x798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc
- PrivKey: 0x0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b

# Dorothy:
- Address: 0x773539d4Ac0e786233D90A233654ccEE26a613D9
- PrivKey: 0x39539ab1876910bbf3a223d84a29e28f1cb4e2e456503e7e91ed39b2e7223d68

# Ethan:
- Address: 0xFf64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB
- PrivKey: 0x7dce9bc8babb68fec1409be38c8e1a52650206a7ed90ff956ae8a6d15eeaaef4

# Faith:
- Address: 0xC0F0f4ab324C46e55D02D0033343B4Be8A55532d
- PrivKey: 0xb9d2ea9a615f3165812e8d44de0d24da9bbd164b65c4f0573e1ce2c8dbd9c8df
```

# Template Runtimes

Frontier provides two different runtimes, one for each strategy.
You can choose which one want to build by using the `--feature` flag. For example:
```
$ cargo build --release                        # this builds a runtime with AccountId32
$ cargo build --release --features accountid32 # this also builds a runtime with AccountId32
$ cargo build --release --features accountid20 # this build a runtime with AccountId20
```
