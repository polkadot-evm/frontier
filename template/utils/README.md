# Substrate EVM Utilities

This directory is home to a Node.js project with some helpful utilities for working with Substrate
and the EVM pallet.

## Installation and Usage

Use `npm i` to install dependencies. To use these utilities, execute
`node ./utils <command> <parameters>` in the project root (i.e. the parent of this folder).

## Commands

This utility supports the following commands:

### `--erc20-slot <slot> <address>`

Calculate the storage slot for an (EVM) address's ERC-20 balance, where `<slot>` is the storage slot
for the ERC-20 balances map

```bash
node ./utils --erc20-slot 0 0xd43593c715fdd31c61141abd04a99fd6822c8558

0x000000000000000000000000d43593c715fdd31c61141abd04a99fd6822c85580000000000000000000000000000000000000000000000000000000000000000
0x045c0350b9cf0df39c4b40400c965118df2dca5ce0fbcf0de4aafc099aea4a14
```

### `--evm-address <address>`

Calculate the **hashed** EVM address that corresponds to a native Substrate address.

```bash
$ node ./utils --evm-address 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
$ 0x57d213d0927ccc7596044c6ba013dd05522aacba
```

> NOTE: the template presently uses the **truncated** H160 address format. Thus this utility is not
> needed. Instead, you should use the leading 20 bytes of the hex encoded address produced by the
> [`subkey` tool](https://docs.substrate.io/v3/tools/subkey):

```bash
subkey inspect "//Alice"

Secret Key URI `//Alice` is account:
  Secret seed:       0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a
  Public key (hex):  0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d
  Public key (SS58): 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
  Account ID:        0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d
  SS58 Address:      5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
```

Thus your truncated public address is: `0xd43593c715fdd31c61141abd04a99fd6822c8558`.

### `---help`

Print a help message for the utility project.
