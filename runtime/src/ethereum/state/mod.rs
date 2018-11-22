use keccak_hasher::{KeccakHasher, KECCAK_EMPTY, KECCAK_NULL_RLP};
use substrate_primitives::Hasher;
use node_primitives::{H160, U256, H256};
use runtime_io;
use rstd::collections::btree_map::BTreeMap;
use cow_like::CowLike;

mod account;
mod io;

pub use self::account::Account;

/// Basic account type.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BasicAccount {
	/// Nonce of the account.
	pub nonce: U256,
	/// Balance of the account.
	pub balance: U256,
	/// Storage root of the account.
	pub storage_root: H256,
	/// Code hash of the account.
	pub code_hash: H256,
}

impl Default for BasicAccount {
	fn default() -> Self {
		Self {
			nonce: U256::zero(),
			balance: U256::zero(),
			storage_root: H256::from(&KECCAK_NULL_RLP),
			code_hash: H256::from(&KECCAK_EMPTY),
		}
	}
}

pub struct State {
	overlays: Vec<BTreeMap<H160, Option<Account>>>,
}

impl State {
	pub fn new() -> Self {
		let mut overlays = Vec::new();
		overlays.push(BTreeMap::new());

		State { overlays }
	}

	pub fn checkpoint(&mut self) {
		self.overlays.push(BTreeMap::new());
	}

	pub fn discard_checkpoint(&mut self) {
		if self.overlays.len() < 2 {
			// We must have at least two overlays so that the last checkpoint can be discarded.
			return;
		}

		let last = self.overlays.pop().expect("overlay length checked to be at least 2; qed");
		for (address, account) in last {
			self.overlays.last_mut().expect("overlay length checked to be at least 2; qed").insert(address, account);
		}
	}

	pub fn revert_to_checkpoint(&mut self) {
		if self.overlays.len() < 2 {
			// We must have at least two overlays so that the last checkpoint can be reverted.
			return;
		}

		self.overlays.pop();
	}

	pub fn overlay_account(&self, mut index: usize, address: &H160) -> CowLike<Option<Account>, Option<Account>> {
		if index >= self.overlays.len() {
			index = self.overlays.len() - 1;
		}

		for i in (0..(index + 1)).rev() {
			if let Some(account) = self.overlays[i].get(address) {
				return CowLike::Borrowed(account)
			}
		}

		CowLike::Owned(Account::new(*address))
	}

	pub fn overlay_account_mut(&mut self, mut index: usize, address: &H160) -> &mut Option<Account> {
		if index >= self.overlays.len() {
			index = self.overlays.len() - 1;
		}

		if !self.overlays[index].contains_key(address) {
			let mut found: Option<Option<Account>> = None;

			for i in (0..index).rev() {
				if let Some(account) = self.overlays[i].get(address) {
					found = Some(account.clone());
					break;
				}
			}

			self.overlays[index].insert(*address, found.unwrap_or(Account::new(*address)));
		}

		self.overlays[index].get_mut(address).expect("Account already exists or has just been inserted; qed")
	}

	pub fn account(&self, address: &H160) -> CowLike<Option<Account>, Option<Account>> {
		self.overlay_account(self.overlays.len() - 1, address)
	}

	pub fn account_mut(&mut self, address: &H160) -> &mut Option<Account> {
		let index = self.overlays.len() - 1;
		self.overlay_account_mut(index, address)
	}
}
