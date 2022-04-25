# Functional testing for Substrate Frontier Node RPC

This folder contains a set of functional tests designed to perform functional testing on the Frontier Eth RPC.

It is written in typescript, using Mocha/Chai as Test framework.

## Test flow

Tests are separated depending on their genesis requirements.
Each group will start a `frontier test node` with a given `spec` before executing the tests.

## Build the manual seal node for tests

```bash
cargo build --release --no-default-features --features manual-seal,rpc_binary_search_estimate
```

## Installation

```bash
npm install
```

## Run the tests

```bash
npm run build && npm run test
```

You can also add the Frontier Node logs to the output using the `FRONTIER_LOG` env variable. Ex:

```bash
FRONTIER_LOG="warn,rpc=trace" npm run test
```

(The frontier node be listening for RPC on port 19933, mostly to avoid conflict with already running substrate node)
