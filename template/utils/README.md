# Substrate EVM Utilities

This directory is home to a Node.js project with some helpful utilities for working with Substrate and the EVM pallet.

## Installation and Usage

Use `npm i` to install dependencies. To use these utilities, execute `node ./utils <command> <parameters>` in the project root (i.e. the parent of this folder).

## Commands

This utility supports the following commands:

### `--erc20-slot <slot> <address>`

Calculate the storage slot for an (EVM) address's ERC-20 balance, where `<slot>` is the storage slot for the ERC-20 balances map

```
$ node ./utils --erc20-slot 0 0x57d213d0927ccc7596044c6ba013dd05522aacba
$ 0xa7473b24b6fd8e15602cfb2f15c6a2e2770a692290d0c5097b77dd334132b7ce
```

### `--evm-address <address>`

Calculate the EVM address that corresponds to a native Substrate address.

```
$ node ./utils --evm-address 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
$ 0x57d213d0927ccc7596044c6ba013dd05522aacba
```

### `---help`

Print a help message for the utility project.
