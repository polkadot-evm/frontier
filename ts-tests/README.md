# Functional testing for Substrate Frontier Node RPC

This folder contains a set of functional tests designed to perform functional testing on the Frontier Eth RPC.

It is written in typescript, using Mocha/Chai as Test framework.

## Test flow

Tests are separated depending on their genesis requirements.
Each group will start a `frontier template test node` with a given `spec` before executing the tests.

## Build the node for tests

```bash
cargo build --release
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

## Attaching to Existing Frontier Node for Tests

The test suite now supports attaching to an already running Frontier node instead of spawning a new one. This is useful for debugging with a node that has a debugger attached.

### Usage

Set the `FRONTIER_ATTACH` environment variable before running tests:

```bash
# Attach to existing node
FRONTIER_ATTACH=true npm test

# Or for a specific test
FRONTIER_ATTACH=true npx mocha -r ts-node/register tests/test-eip7702.ts
```

### Requirements

The existing node must be running with these parameters:
- `--rpc-port=19932` (matches test suite expectations)
- `--sealing=manual` (for controlled block production)
- Other standard test parameters as shown in debug.json

### Example Workflow

1. Start your debug node:
   ```bash
   ./target/debug/frontier-template-node \
     --chain=dev \
     --validator \
     --execution=Native \
     --sealing=manual \
     --no-grandpa \
     --force-authoring \
     --rpc-port=19932 \
     --rpc-cors=all \
     --rpc-methods=unsafe \
     --rpc-external \
     --tmp \
     --unsafe-force-node-key-generation
   ```

2. Run tests with attachment mode:
   ```bash
   cd ts-tests
   FRONTIER_ATTACH=true npm test
   ```
