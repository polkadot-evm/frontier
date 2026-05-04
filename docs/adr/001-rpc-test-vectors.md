# ADR-001: RPC Test Vectors for Ethereum JSON-RPC Compatibility

## Status

Accepted (2026-05-04)

## Context

Frontier provides Ethereum compatibility for Substrate-based blockchains through its RPC layer (`client/rpc-core/` and `client/rpc/`). Currently, RPC testing relies on TypeScript integration tests in `ts-tests/` that spin up a full Frontier node and execute tests via web3.js/ethers.js.

While these integration tests are valuable, they have limitations:

1. **Type-level testing only**: Tests use typed web3.js/ethers.js APIs, which may mask issues with raw JSON-RPC wire format (e.g., field naming like `gasLimit` vs `gas`, hex encoding edge cases).
2. **No cross-client compatibility signal**: There is no mechanism to verify Frontier's RPC responses against the canonical vectors other Ethereum clients (Geth, Reth, Nethermind) are tested with.
3. **Regression detection**: Changes to RPC types in Rust may silently break wire compatibility without being caught.

### How the Ethereum Ecosystem Approaches This

The canonical source of RPC test vectors is [`ethereum/execution-apis`](https://github.com/ethereum/execution-apis), which hosts:

- **OpenRPC schemas** for every JSON-RPC method
- **Test vectors** in `tests/{method}/{test}.io` using a line-delimited request/response format
- **Tooling** in `tools/`: `rpctestgen` (vector generator, run against geth), `speccheck` (validates fixtures against the schema), `specgen` (compiles YAML to OpenRPC)

The standalone `lightclient/rpctestgen` repository was archived on 2026-01-22 and folded into `execution-apis/tools/rpctestgen` — schemas, generator, validator, and vectors now all live in one tree.

The vector format is intentionally simple:

```
>> {"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"}
<< {"jsonrpc":"2.0","id":1,"result":"0x3"}
```

Reth, Geth, and other clients run against these vectors directly — no per-client conversion. Adopting the same source means Frontier inherits new vectors as the upstream evolves, with no fixture maintenance on our side.

## Decision

Add a minimal RPC test-vector runner that replays `ethereum/execution-apis` vectors against a Frontier dev node. Two pieces, no custom tooling:

### 1. Vendored upstream vectors

Add `ethereum/execution-apis` as a git submodule under `client/rpc-test-vectors/vendor/execution-apis/` (or vendor a pinned subset if submodule churn is a concern). Vectors are consumed in their native rpctestgen format — Frontier does **not** define a custom fixture schema.

Pin to a specific commit of `execution-apis`. Refreshing is a deliberate "bump the submodule" PR, the same workflow used for any vendored dependency.

### 2. Test runner crate

A new crate at `client/rpc-test-vectors/` named `fc-rpc-test-vectors`, marked `publish = false` and consumed only as a dev-dependency. Placement matches the existing convention (`fc-rpc`, `fc-rpc-core`, `fc-rpc-v2` all live as siblings under `client/`). The crate:

1. Boots a Frontier dev node in-process (or attaches to one started by the test harness).
2. Walks the vectors directory, parses each `.io` file, sends the `>>` request, and compares against the `<<` response.
3. Allows per-vector overrides for values that are legitimately dynamic on a fresh dev chain (block hashes, timestamps) via a small allow-list file co-located with the runner — not a full schema language.

That's the entire surface area. No fixture generator, no cross-client comparison harness, no custom JSON schema, no separate CI workflow file (it runs as part of the existing `cargo test` job).

### Out of scope (explicitly)

- **Fixture generator CLI** — would require ongoing maintenance and duplicates `execution-apis/tools/rpctestgen` upstream. If we ever need new vectors, we contribute them there.
- **Cross-client comparison tooling** — running Geth/Reth alongside Frontier in CI is high-cost; the upstream vectors already encode the expected behavior.
- **Custom fixture format** — every conversion layer is a maintenance liability and a divergence risk.
- **Hive integration** — Hive assumes a full Ethereum client; Substrate-specific seal/finality semantics don't map cleanly. Revisit only if a concrete need emerges.

## Implementation Plan

### Phase 1: Runner + initial subset

- Add `execution-apis` submodule pinned to a known-good commit.
- Implement the runner crate with rpctestgen `.io` parsing and dev-node replay.
- Wire the dynamic-value allow-list (start with: block hashes, timestamps, `pending` block fields).
- Enable a curated subset of vectors known to be applicable: `eth_blockNumber`, `eth_chainId`, `eth_getBalance`, `eth_getCode`, `eth_getStorageAt`, `eth_getBlockByNumber`, `eth_getBlockByHash`.

### Phase 2: Expand coverage as compatibility allows

- Enable transaction-shaped vectors (`eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_sendRawTransaction`) once the dev-node fixture chain produces matching state.
- Enable execution vectors (`eth_call`, `eth_estimateGas`).
- Document any vectors that are intentionally skipped, with the reason (Substrate divergence, unsupported feature, etc.).

Vectors not in the enabled set are skipped, not failed — keeps the runner green while coverage grows.

## Consequences

### Positive

- **Wire-format validation**: Raw JSON in/out catches serialization regressions the typed TS tests miss.
- **Zero fixture authoring**: We consume what the ecosystem already produces.
- **Low maintenance**: Refreshing vectors is a submodule bump; no generator or schema to maintain.
- **Cross-client signal for free**: Passing the same vectors Reth/Geth pass is a strong compatibility statement.

### Negative

- **Submodule discipline required**: Bumps need review to catch upstream vectors that newly fail (real regression vs. legitimate Substrate divergence).
- **Dynamic-value allow-list is hand-maintained**: Keep it small; if it grows, that's a signal we're papering over real divergence.
- **Some vectors will never apply**: Substrate-specific behavior (manual sealing, finality) means a non-trivial subset will be permanently skipped.

## Alternatives Considered

### Custom JSON fixture format

Rejected. Any format we invent is a format we maintain and a divergence from upstream. The rpctestgen format is good enough and keeps us in lockstep with the rest of the ecosystem.

### Generate fixtures from a running Frontier node

Rejected for the initial implementation. Self-generated fixtures only assert "Frontier matches Frontier" — they do not validate Ethereum compatibility, which is the actual goal. Useful only if we later want to publish Frontier-as-reference fixtures, which is not a current need.

### Extend TypeScript tests with raw JSON

Considered. Rejected because the vectors are upstream and language-agnostic; consuming them from Rust keeps them next to the RPC implementation and avoids a Node runtime dependency in the test path. The TS suite remains valuable for behavioral/end-to-end coverage.

## References

- [ethereum/execution-apis](https://github.com/ethereum/execution-apis) — canonical schemas, test vectors (`tests/`), and tooling (`tools/{rpctestgen,speccheck,specgen}`)
- [Reth RPC tests](https://github.com/paradigmxyz/reth/tree/main/crates/rpc) — reference for how a Rust client consumes these vectors
- [Frontier TypeScript Tests](../../ts-tests/)
