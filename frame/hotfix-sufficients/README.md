# Hotfix sufficients pallet

The Hotfix sufficients pallet allows hotfixing account inconsistency to patch existing accounts that have a non-zero `nonce` but a zero `sufficients` value.
The accounts' `sufficients` values also need to be non-zero to be consistent.

## Description

This pallet can be used to hotfix the account state where a previous bug in EVM create account, lead to accounts being created with `0` references (consumers + providers + sufficients)
but a non-zero nonce.

The dispatchable `hotfix_inc_account_sufficients` fixes this by taking a list of account addresses that have zero reference counts (consumers, providers, sufficients are all `0`),
and incrementing their `sufficients` reference counter.

Any addresses that do not have a zero reference count, will be unaffected.

License: Apache-2.0
