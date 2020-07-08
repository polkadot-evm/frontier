# Substrate Frontier Node Temlate

A [FRAME](https://substrate.dev/docs/en/next/conceptual/runtime/frame)-based [Substrate](https://substrate.dev/en/) node with the Ethereum RPC support, ready for hacking :rocket:

## Upstream

This template was forked from the [Substrate Node Template](https://github.com/substrate-developer-hub/substrate-node-template). You can find more information on features on this template there.

## Build & Run

To build the chain, execute the following commands from the project root:

```
$ cargo build --release
```

To execute the chain, run:

```
$ ./target/debug/frontier-template-node --dev
```

### Docker image

You can run the frontier node (for development) within Docker directly.  
The Dockerfile is optimized for development speed.  
(Running the `docker run...` command will recompile the binaries but not the dependencies)

Building (takes 5-10 min):
```bash
docker build -t frontier-node-dev .
```

Running (takes 1 min to rebuild binaries):
```bash
docker run -t frontier-node-dev
```

## Genesis Configuration

The development [chain spec](/src/chain_spec.rs) included with this project defines a genesis block that has been pre-configured with an EVM account for [Alice](https://substrate.dev/docs/en/next/development/tools/subkey#well-known-keys). When [a development chain is started](https://github.com/substrate-developer-hub/substrate-node-template#run), Alice's EVM account will be funded with a large amount of Ether (`U256::MAX`).
The [Polkadot UI](https://polkadot.js.org/apps/#?rpc=ws://127.0.0.1:9944) can be used to see the details of Alice's EVM account.
In order to view an EVM account, use the `Developer` tab of the Polkadot UI `Settings` app to define the EVM `Account` type as below.
It is also necessary to define the `Address` and `LookupSource` to send transaction:

```json
  "Address": "AccountId",
  "LookupSource": "AccountId",
  "Account": {
    "nonce": "U256",
    "balance": "U256"
  }
```

Use the `Chain State` app's `Storage` tab to query `evm > accounts` with Alice's EVM account ID (`0x57d213d0927ccc7596044c6ba013dd05522aacba`); the value that is returned should be:
```json
{
  nonce: 0,
  balance: 115,792,089,237,316,195,423,570,985,008,687,907,853,269,984,665,640,564,039,457,584,007,913,129,639,935
}
```

> Further reading: [EVM accounts](https://github.com/danforbes/danforbes/blob/master/writings/eth-dev.md#Accounts)

Alice's EVM account ID was calculated using [a provided utility](/utils/README.md#--evm-address-address).

## Example 1: ERC20 Contract Deployment using EVM dispatchable

The following steps are also available as a [Typescript script](examples/contract-erc20) using Polkadot JS SDK

### Step 1: Contract creation

The [`truffle`](examples/contract-erc20/truffle) directory contains a [Truffle](https://www.trufflesuite.com/truffle) project that defines [an ERC-20 token](examples/contract-erc20/truffle/contracts/MyToken.sol). For convenience, this repository also contains [the compiled bytecode of this token contract](examples/contract-erc20/truffle/build/contracts/MyToken.json#L259), which can be used to deploy it to the Substrate blockchain.

> Further reading: [the ERC-20 token standard](https://github.com/danforbes/danforbes/blob/master/writings/eth-dev.md#EIP-20-ERC-20-Token-Standard)

Use the Polkadot UI `Extrinsics` app to deploy the contract from Alice's account (submit the extrinsic as a signed transaction) using `evm > create` with the following parameters:

```
init: <contract bytecode>
value: 0
gas_limit: 4294967295
gas_price: 1
```

The values for `gas_limit` and `gas_price` were chosen for convenience and have little inherent or special meaning.

While the extrinsic is processing, open the browser console and take note of the output. Once the extrinsic has finalized, the EVM pallet will fire a `Created` event with an `address` field that provides the address of the newly-created contract. In this case, however, it is trivial to [calculate this value](https://ethereum.stackexchange.com/a/46960): `0x11650d764feb44f78810ef08700c2284f7e81dcb`. That is because EVM contract account IDs are determined solely by the ID and nonce of the contract creator's account and, in this case, both of those values are well-known (`0x57d213d0927ccc7596044c6ba013dd05522aacba` and `0x0`, respectively).

Use the `Chain State` app to view the EVM accounts for Alice and the newly-created contract; notice that Alice's `nonce` has been incremented to `1` and her `balance` has decreased. Next, query `evm > accountCodes` for both Alice's and the contract's account IDs; notice that Alice's account code is empty and the contract's is equal to the bytecode of the Solidity contract.

### Step 2: Check Contract Storage

The ERC-20 contract that was deployed inherits from [the OpenZeppelin ERC-20 implementation](https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC20/ERC20.sol) and extends its capabilities by adding [a constructor that mints a maximum amount of tokens to the contract creator](truffle/contracts/MyToken.sol#L8). Use the `Chain State` app to query `evm > accountStorage` and view the value associated with Alice's account in the `_balances` map of the ERC-20 contract; use the ERC-20 contract address (`0x11650d764feb44f78810ef08700c2284f7e81dcb`) as the first parameter and the storage slot to read as the second parameter (`0xa7473b24b6fd8e15602cfb2f15c6a2e2770a692290d0c5097b77dd334132b7ce`). The value that is returned should be `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff`.

The storage slot was calculated using [a provided utility](utils/README.md#--erc20-slot-slot-address). (Slot 0 and alice address: `0x57d213d0927ccc7596044c6ba013dd05522aacba`)

> Further reading: [EVM layout of state variables in storage](https://solidity.readthedocs.io/en/v0.6.2/miscellaneous.html#layout-of-state-variables-in-storage)

### Step 3: Contract Usage

Use the `Extrinsics` app to invoke the `transfer(address, uint256)` function on the ERC-20 contract with `evm > call` and transfer some of the ERC-20 tokens from Alice to Bob.

```
target: 0x11650d764feb44f78810ef08700c2284f7e81dcb
input: 0xa9059cbb0000000000000000000000008bc395119f39603402d0c85bc9eda5dfc5ae216000000000000000000000000000000000000000000000000000000000000000dd
value: 0
gas_limit: 4294967295
gas_price: 1
```

The value of the `input` parameter is an EVM ABI-encoded function call that was calculated using [the Remix web IDE](http://remix.ethereum.org); it consists of a function selector (`0xa9059cbb`) and the arguments to be used for the function invocation. In this case, the arguments correspond to Bob's EVM account ID (`0x8bc395119f39603402d0c85bc9eda5dfc5ae2160`) and the number of tokens to be transferred (`0xdd`, or 221 in hex).

> Further reading: [the EVM ABI specification](https://solidity.readthedocs.io/en/v0.6.2/abi-spec.html)

### Step 4: Check Bob Contract Storage

After the extrinsic has finalized, use the `Chain State` app to query `evm > accountStorage` to see the ERC-20 balances for both Alice and Bob.
