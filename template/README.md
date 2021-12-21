# Substrate Frontier Node Template

A [FRAME](https://docs.substrate.io/v3/runtime/frame)-based
[Substrate](https://substrate.io) node with the Ethereum RPC support, ready for hacking
:rocket:

## Generation & Upstream

This template is maintained in the
[Frontier](https://github.com/paritytech/frontier/tree/master/template) project repository, and can
be used to generate a stand-alone template for use in an independent project via the included
[template generation script](https://github.com/paritytech/frontier/blob/master/docs/node-template-release.md).

A ready-to-use template generated this way is hosted for each Frontier release on the
[substrate-developer-hub/frontier-node-template](https://github.com/substrate-developer-hub/frontier-node-template)
repository.

This template was originally forked from the
[Substrate Node Template](https://github.com/substrate-developer-hub/substrate-node-template). You
can find more information on features on this template there, and more detailed usage on the
[Substrate Developer Hub Tutorials](https://docs.substrate.io/tutorials/v3/) that use this heavily.

## Build & Run

To build the chain, execute the following commands from the project root:

```
$ cargo build --release
```

To execute the chain, run:

```
$ ./target/debug/frontier-template-node --dev
```

The node also supports to use manual seal (to produce block manually through RPC).  
This is also used by the ts-tests:

```
$ ./target/debug/frontier-template-node --dev --manual-seal
```

### Docker Based Development

Optionally, You can build and run the frontier node within Docker directly.  
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

The development [chain spec](node/src/chain_spec.rs) included with this project defines a genesis
block that has been pre-configured with an EVM account for
[Alice](https://docs.substrate.io/v3/tools/subkey#well-known-keys). When
[a development chain is started](https://github.com/substrate-developer-hub/substrate-node-template#run),
Alice's EVM account will be funded with a large amount of Ether. The
[Polkadot UI](https://polkadot.js.org/apps/#?rpc=ws://127.0.0.1:9944) can be used to see the details
of Alice's EVM account. In order to view an EVM account, use the `Developer` tab of the Polkadot UI
`Settings` app to define the EVM `Account` type as below. It is also necessary to define the
`Address` and `LookupSource` to send transaction, and `Transaction` and `Signature` to be able to
inspect blocks:

```json
{
	"Address": "MultiAddress",
	"LookupSource": "MultiAddress",
	"Account": {
		"nonce": "U256",
		"balance": "U256"
	},
	"Transaction": {
		"nonce": "U256",
		"action": "String",
		"gas_price": "u64",
		"gas_limit": "u64",
		"value": "U256",
		"input": "Vec<u8>",
		"signature": "Signature"
	},
	"Signature": {
		"v": "u64",
		"r": "H256",
		"s": "H256"
	}
}
```

Use the `Developer` app's `RPC calls` tab to query `eth > getBalance(address, number)` with Alice's
EVM account ID (`0xd43593c715fdd31c61141abd04a99fd6822c8558`); the value that is returned should be:

```text
x: eth.getBalance
340,282,366,920,938,463,463,374,607,431,768,211,455
```

> Further reading:
> [EVM accounts](https://github.com/danforbes/danforbes/blob/master/writings/eth-dev.md#Accounts)

Alice's EVM account ID was calculated using
[an included utility script](utils/README.md#--evm-address-address).

## Example 1: ERC20 Contract Deployment using EVM dispatchable

The following steps are also available as a [Typescript script](examples/contract-erc20) using
Polkadot JS SDK

### Step 1: Contract creation

The [`truffle`](examples/contract-erc20/truffle) directory contains a
[Truffle](https://www.trufflesuite.com/truffle) project that defines
[an ERC-20 token](examples/contract-erc20/truffle/contracts/MyToken.sol). For convenience, this
repository also contains
[the compiled bytecode of this token contract](examples/contract-erc20/truffle/contracts/MyToken.json#L259),
which can be used to deploy it to the Substrate blockchain.

> Further reading:
> [the ERC-20 token standard](https://github.com/danforbes/danforbes/blob/master/writings/eth-dev.md#EIP-20-ERC-20-Token-Standard)

Use the Polkadot UI `Extrinsics` app to deploy the contract from Alice's account (submit the
extrinsic as a signed transaction) using `evm > create` with the following parameters:

```
source: 0xd43593c715fdd31c61141abd04a99fd6822c8558
init: <raw contract bytecode, a very long hex value>
value: 0
gas_limit: 4294967295
gas_price: 1
nonce: <empty> {None}
```

The values for `gas_limit` and `gas_price` were chosen for convenience and have little inherent or
special meaning. Note that `None` for the nonce will increment the known nonce for the source
account, starting from `0x0`, you may manually set this but will get an "evm.InvalidNonce" error if
not set correctly.

Once the extrinsic is in a block, navigate to the `Network` -> `Explorer` tab in the UI, or open up
the browser console to see that the EVM pallet has fired a `Created` event with an `address` field
that provides the address of the newly-created contract:

```bash
# console:
... {"phase":{"applyExtrinsic":2},"event":{"index":"0x0901","data":["0x8a50db1e0f9452cfd91be8dc004ceb11cb08832f"]} ...

# UI:
evm.Created
A contract has been created at given [address]
   H160: 0x8a50db1e0f9452cfd91be8dc004ceb11cb08832f
```

In this case, however, it is trivial to
[calculate this value](https://ethereum.stackexchange.com/a/46960):
`0x8a50db1e0f9452cfd91be8dc004ceb11cb08832f`. That is because EVM contract account IDs are
determined solely by the ID and nonce of the contract creator's account and, in this case, both of
those values are well-known (`0xd43593c715fdd31c61141abd04a99fd6822c8558` and `0x0`, respectively).

### Step 2: Check Contract Storage

Use the `Chain State` UI tab to query`evm > accountCodes` for both Alice's and the contract's
account IDs; notice that Alice's account code is empty and the contract's is equal to the bytecode
of the Solidity contract.

The ERC-20 contract that was deployed inherits from
[the OpenZeppelin ERC-20 implementation](https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/token/ERC20/ERC20.sol)
and extends its capabilities by adding
[a constructor that mints a maximum amount of tokens to the contract creator](examples/contract-erc20/truffle/contracts/MyToken.sol#L8).
Use the `Chain State` app to query `evm > accountStorage` and view the value associated with Alice's
account in the `_balances` map of the ERC-20 contract; use the ERC-20 contract address
(`0x8a50db1e0f9452cfd91be8dc004ceb11cb08832f`) as the first parameter and the storage slot to read
as the second parameter (`0x045c0350b9cf0df39c4b40400c965118df2dca5ce0fbcf0de4aafc099aea4a14`). The
value that is returned should be
`0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff`.

The storage slot was calculated using
[a provided utility](utils/README.md#--erc20-slot-slot-address). (Slot 0 and alice address:
`0xd43593c715fdd31c61141abd04a99fd6822c8558`)

> Further reading:
> [EVM layout of state variables in storage](https://solidity.readthedocs.io/en/latest/miscellaneous.html#layout-of-state-variables-in-storage)

### Step 3: Contract Usage

Use the `Developer` -> `Extrinsics` tab to invoke the `transfer(address, uint256)` function on the
ERC-20 contract with `evm > call` and transfer some of the ERC-20 tokens from Alice to Bob.

```text
target: 0x8a50db1e0f9452cfd91be8dc004ceb11cb08832f
source: 0xd43593c715fdd31c61141abd04a99fd6822c8558
input: 0xa9059cbb0000000000000000000000008eaf04151687736326c9fea17e25fc528761369300000000000000000000000000000000000000000000000000000000000000dd
value: 0
gas_limit: 4294967295
gas_price: 1
```

The value of the `input` parameter is an EVM ABI-encoded function call that was calculated using
[the Remix web IDE](http://remix.ethereum.org); it consists of a function selector (`0xa9059cbb`)
and the arguments to be used for the function invocation. In this case, the arguments correspond to
Bob's EVM account ID (`0x8eaf04151687736326c9fea17e25fc5287613693`) and the number of tokens to be
transferred (`0xdd`, or 221 in hex).

> Further reading:
> [the EVM ABI specification](https://solidity.readthedocs.io/en/latest/abi-spec.html)

### Step 4: Check Bob Contract Storage

After the extrinsic has finalized, use the `Chain State` app to query `evm > accountStorage` to see
the ERC-20 balances for both Alice and Bob.
