use super::io;
use node_primitives::{H160, H256, U256};
use rstd::collections::btree_map::BTreeMap;
use rstd::mem;
use keccak_hasher::{KECCAK_EMPTY, KECCAK_NULL_RLP};

#[derive(Clone)]
pub struct Account {
	address: H160,
	base_storage_root: H256,
	storage_changes: BTreeMap<H256, H256>,
	nonce: U256,
	balance: U256,
	code_hash: H256,
	code: Vec<u8>,
}

impl Account {
	pub fn new(address: H160) -> Option<Account> {
		match io::read_account(&address) {
			Some(basic) => {
				let code = io::read_account_code(&basic.code_hash).expect("Non-corrupt database will not read non-existant code hash; qed");
				Some(Account {
					address,
					code,
					base_storage_root: basic.storage_root,
					storage_changes: BTreeMap::new(),
					nonce: basic.nonce,
					balance: basic.balance,
					code_hash: basic.code_hash
				})
			},
			None => None,
		}
	}

	pub fn initialise(address: H160, balance: Option<U256>, nonce: U256) -> Account {
		match io::read_account(&address) {
			Some(basic) => {
				Account {
					address,
					nonce,
					code: Vec::new(),
					code_hash: H256::from(&KECCAK_EMPTY),
					base_storage_root: H256::from(&KECCAK_NULL_RLP),
					storage_changes: BTreeMap::new(),
					balance: balance.unwrap_or(basic.balance),
				}
			},
			None => {
				Account {
					address,
					nonce,
					code: Vec::new(),
					code_hash: H256::from(&KECCAK_EMPTY),
					base_storage_root: H256::from(&KECCAK_NULL_RLP),
					storage_changes: BTreeMap::new(),
					balance: balance.unwrap_or(U256::zero()),
				}
			},
		}
	}

	pub fn commit(&mut self) {
		let mut basic = io::read_account(&self.address).unwrap_or_default();

		// Commit code
		io::note_account_code(&self.code_hash, &self.code);
		basic.code_hash = self.code_hash;

		// Commit storage
		if basic.storage_root != self.base_storage_root {
			assert!(self.base_storage_root == H256::from(KECCAK_NULL_RLP));
			io::kill_account_storage(&self.address);
		}
		let mut storage_changes = BTreeMap::new();
		mem::swap(&mut storage_changes, &mut self.storage_changes);
		for (key, val) in storage_changes {
			io::write_account_storage(&self.address, key, if val == H256::default() {
				None
			} else {
				Some(val)
			});
		}
		self.base_storage_root = io::account_storage_root(&self.address);
		basic.storage_root = self.base_storage_root;

		// Commit self itself
		basic.nonce = self.nonce;
		basic.balance = self.balance;
		io::write_account(self.address, Some(basic));
	}

	pub fn kill(address: &H160) {
		io::kill_account_storage(address);
		io::write_account(*address, None);
	}
}
