# Changelog for `fc-rpc-core`

## Unreleased
- Add `FilterPoolItem::log_scan_done`, recording whether a log filter has served its first poll.
- Add `FilteredParams::address_in_bloom()` and `FilteredParams::topics_in_bloom()` functions to check the possible existence of Filter addresses or topics in a block.
- Removed `PendingTransaction` and `PendingTransactions` types.
