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

use fp_evm::InvalidEvmTransactionError as VError;
use sc_transaction_pool_api::error::{Error as PError, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;

// Formats the same way Geth node formats responses.
pub struct Geth;

impl Geth {
	pub fn pool_error(err: impl IntoPoolError) -> String {
		// Error strings from :
		// https://github.com/ethereum/go-ethereum/blob/794c6133efa2c7e8376d9d141c900ea541790bce/core/error.go
		match err.into_pool_error() {
			Ok(PError::AlreadyImported(_)) => "already known".to_string(),
			// In Frontier the only case there is a `TemporarilyBanned` is because
			// the same transaction was received before and returned
			// `InvalidTransaction::Stale`. Thus we return the same error.
			Ok(PError::TemporarilyBanned) => "nonce too low".into(),
			Ok(PError::TooLowPriority { .. }) => "replacement transaction underpriced".into(),
			Ok(ref outer @ PError::InvalidTransaction(inner)) => match inner {
				InvalidTransaction::Stale => "nonce too low".into(),
				InvalidTransaction::Payment => "insufficient funds for gas * price + value".into(),
				InvalidTransaction::ExhaustsResources => "gas limit reached".into(),
				InvalidTransaction::Custom(inner) => match inner {
					a if a == VError::InvalidChainId as u8 => "invalid chain id".into(),
					// VError::InvalidSignature => "invalid sender".into(),
					a if a == VError::GasLimitTooLow as u8 => "intrinsic gas too low".into(),
					a if a == VError::GasLimitTooHigh as u8 => "exceeds block gas limit".into(),
					_ => format!("submit transaction to pool failed: {:?}", outer),
				},
				_ => format!("submit transaction to pool failed: {:?}", outer),
			},
			err => format!("submit transaction to pool failed: {:?}", err),
		}
	}
}
