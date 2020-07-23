# Functional testing for Substrate Frontier Node RPC

This folder contains a set of functional tests desgined to perform functional testing on the Frontier Eth RPC.

It is written in typescript, using Mocha/Chai as Test framework.

## Test flow

Tests are separated depending of their genesis requirements.
Each group will start a [frontier test node](frontier-test-node) with a given [spec](substrate-specs) before executing the tests.

## Installation

```
npm install
```

## Run the tests

```
npm run test
```

You can also add the Frontier Node logs to the output using the `FRONTIER_LOG` env variable. Ex:

```
FRONTIER_LOG="warn,rpc=trace" npm run test
```

(The frontier node be listening for RPC on port 19933, mostly to avoid conflict with already running substrate node)
