use crate::{
	mock::*, CallOrCreateInfo, Error, RawOrigin, Transaction, TransactionAction, H160, H256, U256,
};
use ethereum::TransactionSignature;
use frame_support::{
	assert_err, assert_noop, assert_ok,
	unsigned::{TransactionValidityError, ValidateUnsigned},
};
use rustc_hex::{FromHex, ToHex};
use sp_runtime::{
	traits::Applyable,
	transaction_validity::{InvalidTransaction, TransactionSource, ValidTransactionBuilder},
};
use std::str::FromStr;

mod eip1559;
mod eip2930;
mod legacy;

// This ERC-20 contract mints the maximum amount of tokens to the contract creator.
// pragma solidity ^0.5.0;`
// import "https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v2.5.1/contracts/token/ERC20/ERC20.sol";
// contract MyToken is ERC20 {
//	 constructor() public { _mint(msg.sender, 2**256 - 1); }
// }
pub const ERC20_CONTRACT_BYTECODE: &str = include_str!("./res/erc20_contract_bytecode.txt");
