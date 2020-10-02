# ERC20 contract creation

This directory contains typescript script describing the different
topics presented by the frontier node template.

## Basic

### Installation and Usage

Use `npm i` to install dependencies. To create an ERC20 contract,
execute `node_modules/.bin/ts-node create-erc20.ts` while your
template node is running in `--dev` mode.

### Expected output

The ouput of the command should look similar to this:

```
└────╼ ts-node create-erc20.ts
Initiating the API (ignore message "Unable to resolve type B..." and "Unknown types found...")
Unable to resolve type B, it will fail on construction
Unknown types found, no types for B, Receipt, Transaction
Initialiation done
Genesis at block: 0x8455be17576e759feb9f027d79185d4e51fe91113185ef9a315614a35f0f86d8
Alice Substrate Account: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
Alice Substrate Account (nonce: 0) balance, free: 0x00000000000000001000000000000000
Alice EVM Account: 0x57d213d0927ccc7596044c6ba013dd05522aacba
Alice EVM Account (nonce: 0) balance: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

Step 1: Creating Smart Contract
Contract creation is Ready
Contract creation is {"InBlock":"0xb7f31941812cfddede61918a219d472d3b953e05b619a6baa2bd6847a6140bad"}
Contract included at blockHash 0xb7f31941812cfddede61918a219d472d3b953e05b619a6baa2bd6847a6140bad
Waiting for finalization... (can take a minute)
Contract creation is {"Finalized":"0xb7f31941812cfddede61918a219d472d3b953e05b619a6baa2bd6847a6140bad"}
Contract finalized at blockHash 0xb7f31941812cfddede61918a219d472d3b953e05b619a6baa2bd6847a6140bad
Contract address: 0x11650d764feb44f78810ef08700c2284f7e81dcb

Step 2: Retrieving Contract from evm address: 0x11650d764feb44f78810ef08700c2284f7e81dcb
Alice Substrate Account (nonce: 1) balance, free: 1152921500186875190
Contract account code: 0x60806040523480...6c63430005100032
Alice Contract storage key: 0xa7473b24b6fd8e15602cfb2f15c6a2e2770a692290d0c5097b77dd334132b7ce
Alice Contract account storage: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

Step 3: Transfering Tokens to Bob EVM Account: 0x8bc395119f39603402d0c85bc9eda5dfc5ae2160
Preparing transfer of 0xdd
Sending call input: 0xa9059cbb0000000000000000000000008bc395119f39603402d0c85bc9eda5dfc5ae216000000000000000000000000000000000000000000000000000000000000000dd
Transfer is Ready
Transfer is {"InBlock":"0x22dcfc5b497bd1ea42ef38a33ebaee959a9fbd4337389c8a6a0961fc0af4910b"}
Transfer included at blockHash 0x22dcfc5b497bd1ea42ef38a33ebaee959a9fbd4337389c8a6a0961fc0af4910b
Waiting for finalization... (can take a minute)
Transfer is {"Finalized":"0x22dcfc5b497bd1ea42ef38a33ebaee959a9fbd4337389c8a6a0961fc0af4910b"}
Transfer finalized at blockHash 0x22dcfc5b497bd1ea42ef38a33ebaee959a9fbd4337389c8a6a0961fc0af4910b

Step 4: Retrieving Bob tokens
Bob Substrate Account (nonce: 0) balance, free: 1152921504606846976
Bob EVM Account: 0x8bc395119f39603402d0c85bc9eda5dfc5ae2160
Bob EVM Account (nonce: 0) balance: 0x0000000000000000000000000000000000000000000000000000000000000000
Bob Contract storage key: 0x0e4b5229940f8e2bf475520e854b789139893f70ee7b5ec9006de746028449fe
Bob Contract account storage: 0x00000000000000000000000000000000000000000000000000000000000000dd
```

## RPC

This section describes how to use the web3.js SDK to interact with
Frontier.

## Installation and Usage

Use `npm i` to install dependencies. To create an ERC20 contract,
execute `node_modules/.bin/ts-node create-erc20.ts` while your
template node is running in `--dev` mode.

## Expected output

WIP
