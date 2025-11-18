## Approach to fuzzing
This fuzzing harness uses a "structure-aware" approach by using [arbitrary](https://github.com/rust-fuzz/arbitrary) to fuzz the Runtime.

The harness has multiple substrate invariants, but two frontier specific ones:
1. The proof size must never exceed the supplied max proof size
4. The execution time MUST be within a reasonable threshold.

Important notes:
1. Since the fuzzing happens in ``debug`` mode, the EVM execution will be notably slower. Set a reasonably high timeout, five or six seconds is a good starter.
2. You will need to use ``SKIP_WASM_BUILD=1`` to fuzz due to some polkadot-sdk wasm conflicts.

## Orchestrating the campaign
Fuzzing is orchestrated by [ziggy](https://github.com/srlabs/ziggy/).

It uses [AFL++](https://github.com/AFLplusplus/AFLplusplus/) and [honggfuzz](https://github.com/google/honggfuzz) under the hood.

Please refer to its documentation for details.

Quickstart command to fuzz:

``` bash
SKIP_WASM_BUILD=1 cargo ziggy fuzz -j$(nproc) -t5
```
