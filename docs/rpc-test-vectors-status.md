# RPC Test Vectors — Status

Snapshot of the `fc-rpc-test-vectors` runner against
`frontier-template-node --dev`, baselined from CI run
`25383306511` against branch `manuel/add-rpc-test-vectors`.

The full skip list lives in
[`client/rpc-test-vectors/vendor-skip.txt`](../client/rpc-test-vectors/vendor-skip.txt).
This document is the human-readable companion: it groups skips into
buckets so a maintainer scanning the list knows which entries are noise
to be classified vs. real bugs to be filed.

## Headline

| Bucket                        | Count | What it means                                  |
| ----------------------------- | ----- | ---------------------------------------------- |
| Match / SchemaOnly            | 103   | passes                                         |
| Excluded namespace            | 4     | `testing_*`, `engine_*` (always skipped)       |
| Skipped via `vendor-skip.txt` | 103   | failures grouped below                         |
| Real failures                 | 0     | should remain 0; new entries here are CI red   |

`103 / (210 − 4) = 50%` of attempted vectors pass.

## Bucket 1 — methods Frontier does not implement (77)

These return JSON-RPC error -32601 (method not found) or a wire shape
Frontier doesn't produce. Whether any are *meant* to be implemented is a
roadmap question; this PR does not take that call. Candidates that are
unlikely to ever be in scope (because they reference Ethereum-specific
internals that don't translate to Substrate):

- `eth_blobBaseFee` — EIP-4844 blob fees; no blob sidecars on Substrate.
- `eth_getProof` — Merkle-Patricia state proofs; Substrate's storage
  trie is different.
- `eth_getStorageValues` — newer batched storage read; same trie issue.
- `debug_getRawBlock`, `debug_getRawHeader`, `debug_getRawReceipts` —
  return geth's RLP-encoded structures byte-for-byte.
- `txpool_content`, `txpool_contentFrom` — txpool inspection methods;
  Frontier's pool inspection differs from geth's response shape.

Could go either way (real gap, but a Substrate impl is conceivable):

- `eth_simulateV1` (64 cases) — newer multi-call simulation API.
- `eth_estimateGas/estimate-with-eip7702` — Pectra authorization-list
  path; depends on EIP-7702 support.

A future PR can promote the "never" set into `EXCLUDED_NAMESPACES` (with
per-method comments) and shrink this bucket.

## Bucket 2 — vectors hard-coded to Hive `chain.rlp` state (14)

These send a specific block hash, signed transaction, or contract call
that exists only in geth's Hive fixture chain. They cannot pass against
`frontier-template-node --dev` regardless of wire-format compatibility,
because the inputs reference state we don't have.

| Method/case                                       | Why it can't pass                            |
| ------------------------------------------------- | -------------------------------------------- |
| `eth_call/call-callenv-options-eip1559`           | calls a contract from Hive chain.rlp         |
| `eth_createAccessList/create-al-abi-revert`       | references contract from Hive chain.rlp     |
| `eth_createAccessList/create-al-contract`         | references contract from Hive chain.rlp     |
| `eth_createAccessList/create-al-contract-eip1559` | references contract from Hive chain.rlp     |
| `eth_createAccessList/create-al-value-transfer`   | references state from Hive chain.rlp        |
| `eth_getBalance/get-balance-blockhash`            | references blockHash from Hive chain.rlp    |
| `eth_getBlockReceipts/get-block-receipts-by-hash` | references blockHash from Hive chain.rlp    |
| `eth_getLogs/filter-with-blockHash`               | references blockHash from Hive chain.rlp    |
| `eth_getLogs/filter-with-blockHash-and-topics`    | references blockHash from Hive chain.rlp    |
| `eth_sendRawTransaction/*` (5 cases)              | signed txs assume Hive chain nonces and keys |

Unlocking this bucket requires a way to seed Frontier from `chain.rlp`
(out of scope for this PR). Until then, these stay skipped.

## Bucket 3 — real Frontier wire-format gaps (12)

These vectors *should* be satisfiable on a dev chain, but Frontier's
response shape diverges from the spec. **These need issues filed** and
the entries removed from `vendor-skip.txt` as fixes land.

| Method/case                                     | Symptom                              | Likely cause                               |
| ----------------------------------------------- | ------------------------------------ | ------------------------------------------ |
| `eth_getBlockByNumber/get-finalized`            | `$.result.blobGasUsed` missing       | Cancun blob fields not exposed             |
| `eth_getBlockByNumber/get-latest`               | `$.result.blobGasUsed` missing       | same                                       |
| `eth_getBlockByNumber/get-safe`                 | `$.result.blobGasUsed` missing       | same                                       |
| `eth_getBlockByNumber/get-genesis`              | `$.result.mixHash` missing           | Substrate genesis omits `mixHash`          |
| `eth_feeHistory/fee-history`                    | `$.result.baseFeePerBlobGas` missing | Cancun blob fields not exposed             |
| `eth_getLogs/filter-error-future-block-range`   | `$.error` missing (returns result)   | doesn't reject invalid ranges              |
| `eth_getLogs/filter-error-reversed-block-range` | `$.error` missing (returns result)   | same                                       |
| `eth_call/call-revert-abi-error`                | `$.error` missing (returns result)   | revert-data not surfaced as JSON-RPC error |
| `eth_call/call-revert-abi-panic`                | `$.error` missing (returns result)   | same                                       |
| `eth_estimateGas/estimate-call-abi-error`       | `$.error` missing (returns result)   | same revert-data path as eth_call          |
| `eth_estimateGas/estimate-failed-call`          | `$.error` missing (returns result)   | same                                       |
| `debug_getRawTransaction/get-invalid-hash`      | `$.error` missing                    | different error shape on invalid hash      |

## How to maintain `vendor-skip.txt`

- **Removing entries**: as a gap closes, delete its line. The next CI run
  against the affected method will then fail loudly if regressions
  reappear.
- **Adding entries**: each new entry needs a one-line reason. If the
  reason is "real bug", file the issue first and link it in the comment.
- **No globs**: case-level only. A new upstream vector for a
  not-implemented method should appear as a fresh failure so we notice
  and decide.
- **Bumping the submodule**: re-run the suite, then triage any new
  failures against the buckets above before adding them.
