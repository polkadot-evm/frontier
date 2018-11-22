use super::io;
use node_primitives::H160;

#[derive(Clone)]
pub struct Account {
	address: H160,
}

impl Account {
	pub fn new(address: H160) -> Option<Account> {
		match io::read_account(address) {
			Some(_) => Some(Account {
				address
			}),
			None => None
		}
	}
}
