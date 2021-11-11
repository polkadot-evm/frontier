use pallet_ethereum::TransactionValidationError as VError;
use sc_transaction_pool_api::error::{Error as PError, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;

// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

// Allows to customize the formating of strings returned by the API.
// This allow to comply with the different formatting various Ethereum
// node implementations have.
pub trait Formatter: Send + Sync + 'static {
	fn pool_error(err: impl IntoPoolError) -> String;
}

// Formatter keeping the same output as before the introduction of this
// formatter design.
pub struct Legacy;

impl Formatter for Legacy {
	fn pool_error(err: impl IntoPoolError) -> String {
		format!("submit transaction to pool failed: {:?}", err)
	}
}

// Formats the same way Geth node formats responses.
pub struct Geth;

impl Formatter for Geth {
	fn pool_error(err: impl IntoPoolError) -> String {
		// Error strings from :
		// https://github.com/ethereum/go-ethereum/blob/794c6133efa2c7e8376d9d141c900ea541790bce/core/error.go
		match err.into_pool_error() {
			Ok(PError::AlreadyImported(_)) => "already known".to_string(),
			// In Frontier the only case there is a `TemporarilyBanned` is because
			// the same transaction was received before and returned
			// `InvalidTransaction::Stale`. Thus we return the same error.
			Ok(PError::TemporarilyBanned) => "nonce too low".into(),
			Ok(PError::TooLowPriority { .. }) => "replacement transaction underpriced".into(),
			Ok(PError::InvalidTransaction(inner)) => match inner {
				InvalidTransaction::Stale => "nonce too low".into(),
				InvalidTransaction::Payment => "insufficient funds for gas * price + value".into(),
				InvalidTransaction::ExhaustsResources => "gas limit reached".into(),
				InvalidTransaction::Custom(inner) => match inner.into() {
					VError::UnknownError => "unknown error".into(),
					VError::InvalidChainId => "invalid chain id".into(),
					VError::InvalidSignature => "invalid sender".into(),
					VError::GasLimitTooLow => "intrinsic gas too low".into(),
					VError::GasLimitTooHigh => "exceeds block gas limit".into(),
					VError::InsufficientFundsForTransfer => {
						"insufficient funds for transfer".into()
					}
				},
				_ => "unknown error".into(),
			},
			err @ _ => format!("submit transaction to pool failed: {:?}", err),
		}
	}
}
