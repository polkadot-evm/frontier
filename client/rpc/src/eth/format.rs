// This file is part of Frontier.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Substrate
use sc_transaction_pool_api::error::{Error as PError, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;
// Frontier
use fp_evm::TransactionValidationError as VError;

// Formats the same way Geth node formats responses.
pub struct Geth;

impl Geth {
	pub fn pool_error(err: impl IntoPoolError) -> String {
		// Error strings from :
		// https://github.com/ethereum/go-ethereum/blob/794c6133efa2c7e8376d9d141c900ea541790bce/core/error.go
		match err.into_pool_error() {
			Ok(PError::AlreadyImported(_)) => "already known".to_string(),
			Ok(PError::TemporarilyBanned) => "already known".into(),
			Ok(PError::TooLowPriority { .. }) => "replacement transaction underpriced".into(),
			Ok(PError::InvalidTransaction(inner)) => match inner {
				InvalidTransaction::Stale => "nonce too low".into(),
				InvalidTransaction::Payment => "insufficient funds for gas * price + value".into(),
				InvalidTransaction::ExhaustsResources => "exceeds block gas limit".into(),
				InvalidTransaction::Custom(inner) => match inner.into() {
					VError::UnknownError => "unknown error".into(),
					VError::InvalidChainId => "invalid chain id".into(),
					VError::InvalidSignature => "invalid sender".into(),
					VError::GasLimitTooLow => "intrinsic gas too low".into(),
					VError::GasLimitTooHigh => "exceeds block gas limit".into(),
					VError::GasPriceTooLow => "gas price less than block base fee".into(),
					VError::PriorityFeeTooHigh => {
						"max priority fee per gas higher than max fee per gas".into()
					}
					VError::InvalidFeeInput => "invalid fee input".into(),
					_ => "transaction validation error".into(),
				},
				_ => "unknown error".into(),
			},
			err => format!("submit transaction to pool failed: {:?}", err),
		}
	}
}
