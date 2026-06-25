# Changelog for `fc-rpc-core`

## Unreleased
- Accept a bare 32-byte block-hash string (e.g. `"0x<64 hex>"`) as the `eth` JSON-RPC block parameter, deserializing it to the `Hash` variant instead of overflowing `u64` parsing.
- Add `FilteredParams::address_in_bloom()` and `FilteredParams::topics_in_bloom()` functions to check the possible existence of Filter addresses or topics in a block.
- Removed `PendingTransaction` and `PendingTransactions` types.
