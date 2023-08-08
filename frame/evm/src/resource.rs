use crate::{AccountCodes, AccountCodesMetadata, Config, Pallet};

use core::marker::PhantomData;
use evm::{
	gasometer::{GasCost, StorageTarget},
	ExitError, Opcode,
};
use fp_evm::WeightInfo;
use sp_core::{Get, H160, H256, U256};
use sp_runtime::{traits::CheckedAdd, Saturating};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// `System::Account` 16(hash) + 20 (key) + 60 (AccountInfo::max_encoded_len)
pub const ACCOUNT_BASIC_PROOF_SIZE: u64 = 96;
/// `AccountCodesMetadata` read, temptatively 16 (hash) + 20 (key) + 40 (CodeMetadata).
pub const ACCOUNT_CODES_METADATA_PROOF_SIZE: u64 = 76;
/// 16 (hash1) + 20 (key1) + 16 (hash2) + 32 (key2) + 32 (value)
pub const ACCOUNT_STORAGE_PROOF_SIZE: u64 = 116;
/// Fixed trie 32 byte hash.
pub const WRITE_PROOF_SIZE: u64 = 32;
/// Account basic proof size + 5 bytes max of `decode_len` call.
pub const IS_EMPTY_CHECK_PROOF_SIZE: u64 = 93;

#[derive(Debug, PartialEq)]
/// Resource error.
pub enum ResourceError {
	/// The Resource usage exceeds the limit.
	LimitExceeded,
	/// Invalid Base Cost.
	InvalidBaseCost,
	///Used to indicate that the code should be unreachable.
	Unreachable,
}

/// A struct that keeps track of resource usage and limit.
pub struct Resource<T> {
	limit: T,
	usage: T,
}

impl<T> Resource<T>
where
	T: CheckedAdd + Saturating + PartialOrd + Copy,
{
	/// Creates a new `Resource` instance with the given base cost and limit.
	///
	/// # Errors
	///
	/// Returns `ResourceError::InvalidBaseCost` if the base cost is greater than the limit.
	pub fn new(base_cost: T, limit: T) -> Result<Self, ResourceError> {
		if base_cost > limit {
			return Err(ResourceError::InvalidBaseCost);
		}
		Ok(Self {
			limit,
			usage: base_cost,
		})
	}

	/// Records the cost of an operation and updates the usage.
	///
	/// # Errors
	///
	/// Returns `ResourceError::LimitExceeded` if the Resource usage exceeds the limit.
	fn record_cost(&mut self, cost: T) -> Result<(), ResourceError> {
		let usage = self
			.usage
			.checked_add(&cost)
			.ok_or(ResourceError::LimitExceeded)?;

		if usage > self.limit {
			return Err(ResourceError::LimitExceeded);
		}
		self.usage = usage;
		Ok(())
	}

	/// Refunds the given amount.
	fn refund(&mut self, amount: T) {
		self.usage = self.usage.saturating_sub(amount);
	}

	/// Returns the usage.
	fn usage(&self) -> T {
		self.usage
	}
}

pub enum AccessedStorage {
	AccountCodes(H160),
	AccountStorages((H160, H256)),
}

#[derive(Default, Clone, Eq, PartialEq)]
pub struct Recorded {
	account_codes: Vec<H160>,
	account_storages: BTreeMap<(H160, H256), bool>,
}

/// A struct that keeps track of the proof size and limit.
pub struct ProofSizeMeter<T> {
	resource: Resource<u64>,
	recorded: Recorded,
	_marker: PhantomData<T>,
}

impl<T: Config> ProofSizeMeter<T> {
	/// Creates a new `ProofSizeResource` instance with the given limit.
	pub fn new(base_cost: u64, limit: u64) -> Result<Self, ResourceError> {
		Ok(Self {
			resource: Resource::new(base_cost, limit)?,
			recorded: Recorded::default(),
			_marker: PhantomData,
		})
	}

	/// Records the size of the proof and updates the usage.
	///
	/// # Errors
	///
	/// Returns `ResourceError::LimitExceeded` if the proof size exceeds the limit.
	pub fn record_proof_size(&mut self, size: u64) -> Result<(), ResourceError> {
		self.resource.record_cost(size)
	}

	/// Refunds the given amount of proof size.
	pub fn refund(&mut self, amount: u64) {
		self.resource.refund(amount)
	}

	/// Returns the proof size usage.
	pub fn usage(&self) -> u64 {
		self.resource.usage()
	}

	/// Returns the proof size limit.
	pub fn limit(&self) -> u64 {
		self.resource.limit
	}

	pub fn record_external_operation(
		&mut self,
		op: &evm::ExternalOperation,
		contract_size_limit: u64,
	) -> Result<(), ResourceError> {
		match op {
			evm::ExternalOperation::AccountBasicRead => {
				self.record_proof_size(ACCOUNT_BASIC_PROOF_SIZE)?
			}
			evm::ExternalOperation::AddressCodeRead(address) => {
				let maybe_record = !self.recorded.account_codes.contains(&address);
				// Skip if the address has been already recorded this block
				if maybe_record {
					// First we record account emptiness check.
					// Transfers to EOAs with standard 21_000 gas limit are able to
					// pay for this pov size.
					self.record_proof_size(IS_EMPTY_CHECK_PROOF_SIZE)?;

					if <AccountCodes<T>>::decode_len(address).unwrap_or(0) == 0 {
						return Ok(());
					}
					// Try to record fixed sized `AccountCodesMetadata` read
					// Tentatively 16 + 20 + 40
					self.record_proof_size(ACCOUNT_CODES_METADATA_PROOF_SIZE)?;
					if let Some(meta) = <AccountCodesMetadata<T>>::get(address) {
						self.record_proof_size(meta.size)?;
					} else {
						// If it does not exist, try to record `create_contract_limit` first.
						self.record_proof_size(contract_size_limit)?;
						let meta = Pallet::<T>::account_code_metadata(*address);
						let actual_size = meta.size;
						// Refund if applies
						self.refund(contract_size_limit.saturating_sub(actual_size));
					}
					self.recorded.account_codes.push(*address);
				}
			}
			evm::ExternalOperation::IsEmpty => self.record_proof_size(IS_EMPTY_CHECK_PROOF_SIZE)?,
			evm::ExternalOperation::Write => self.record_proof_size(WRITE_PROOF_SIZE)?,
		};
		Ok(())
	}

	pub fn record_external_dynamic_opcode_cost(
		&mut self,
		opcode: Opcode,
		target: evm::gasometer::StorageTarget,
		contract_size_limit: u64,
	) -> Result<(), ResourceError> {
		// If account code or storage slot is in the overlay it is already accounted for and early exit
		let mut accessed_storage: Option<AccessedStorage> = match target {
			StorageTarget::Address(address) => {
				if self.recorded.account_codes.contains(&address) {
					return Ok(());
				} else {
					Some(AccessedStorage::AccountCodes(address))
				}
			}
			StorageTarget::Slot(address, index) => {
				if self
					.recorded
					.account_storages
					.contains_key(&(address, index))
				{
					return Ok(());
				} else {
					Some(AccessedStorage::AccountStorages((address, index)))
				}
			}
			_ => None,
		};

		let mut maybe_record_and_refund = |with_empty_check: bool| -> Result<(), ResourceError> {
			let address = if let Some(AccessedStorage::AccountCodes(address)) = accessed_storage {
				address
			} else {
				// This must be unreachable, a valid target must be set.
				// TODO decide how do we want to gracefully handle.
				return Err(ResourceError::Unreachable);
			};
			// First try to record fixed sized `AccountCodesMetadata` read
			// Tentatively 20 + 8 + 32
			let mut base_cost = ACCOUNT_CODES_METADATA_PROOF_SIZE;
			if with_empty_check {
				base_cost = base_cost.saturating_add(IS_EMPTY_CHECK_PROOF_SIZE);
			}
			self.record_proof_size(base_cost)?;
			if let Some(meta) = <AccountCodesMetadata<T>>::get(address) {
				self.record_proof_size(meta.size)?;
			} else {
				// If it does not exist, try to record `create_contract_limit` first.
				self.record_proof_size(contract_size_limit)?;
				let meta = Pallet::<T>::account_code_metadata(address);
				let actual_size = meta.size;
				// Refund if applies
				self.refund(contract_size_limit.saturating_sub(actual_size));
			}
			self.recorded.account_codes.push(address);
			// Already recorded, return
			Ok(())
		};

		// Proof size is fixed length for writes (a 32-byte hash in a merkle trie), and
		// the full key/value for reads. For read and writes over the same storage, the full value
		// is included.
		// For cold reads involving code (call, callcode, staticcall and delegatecall):
		//	- We depend on https://github.com/paritytech/frontier/pull/893
		//	- Try to get the cached size or compute it on the fly
		//	- We record the actual size after caching, refunding the difference between it and the initially deducted
		//	contract size limit.
		let opcode_proof_size = match opcode {
			// Basic account fixed length
			Opcode::BALANCE => {
				accessed_storage = None;
				U256::from(ACCOUNT_BASIC_PROOF_SIZE)
			}
			Opcode::EXTCODESIZE | Opcode::EXTCODECOPY | Opcode::EXTCODEHASH => {
				return maybe_record_and_refund(false)
			}
			Opcode::CALLCODE | Opcode::CALL | Opcode::DELEGATECALL | Opcode::STATICCALL => {
				return maybe_record_and_refund(true)
			}
			// (H160, H256) double map blake2 128 concat key size (68) + value 32
			Opcode::SLOAD => U256::from(ACCOUNT_STORAGE_PROOF_SIZE),
			Opcode::SSTORE => {
				let (address, index) =
					if let Some(AccessedStorage::AccountStorages((address, index))) =
						accessed_storage
					{
						(address, index)
					} else {
						// This must be unreachable, a valid target must be set.
						// TODO decide how do we want to gracefully handle.
						return Err(ResourceError::Unreachable);
					};
				let mut cost = WRITE_PROOF_SIZE;
				let maybe_record = !self
					.recorded
					.account_storages
					.contains_key(&(address, index));
				// If the slot is yet to be accessed we charge for it, as the evm reads
				// it prior to the opcode execution.
				// Skip if the address and index has been already recorded this block.
				if maybe_record {
					cost = cost.saturating_add(ACCOUNT_STORAGE_PROOF_SIZE);
				}
				U256::from(cost)
			}
			// Fixed trie 32 byte hash
			Opcode::CREATE | Opcode::CREATE2 => U256::from(WRITE_PROOF_SIZE),
			// When calling SUICIDE a target account will receive the self destructing
			// address's balance. We need to account for both:
			//	- Target basic account read
			//	- 5 bytes of `decode_len`
			Opcode::SUICIDE => {
				accessed_storage = None;
				U256::from(IS_EMPTY_CHECK_PROOF_SIZE)
			}
			// Rest of dynamic opcodes that do not involve proof size recording, do nothing
			_ => return Ok(()),
		};

		if opcode_proof_size > U256::from(u64::MAX) {
			self.record_proof_size(self.limit())?;
			return Err(ResourceError::LimitExceeded);
		}

		// Cache the storage access
		match accessed_storage {
			Some(AccessedStorage::AccountStorages((address, index))) => {
				self.recorded
					.account_storages
					.insert((address, index), true);
			}
			Some(AccessedStorage::AccountCodes(address)) => {
				self.recorded.account_codes.push(address);
			}
			_ => {}
		}

		// Record cost
		self.record_proof_size(opcode_proof_size.low_u64())?;
		Ok(())
	}

	pub fn record_external_static_opcode_cost(
		&mut self,
		_opcode: Opcode,
		_gas_cost: GasCost,
	) -> Result<(), ResourceError> {
		Ok(())
	}
}

/// A struct that keeps track of the ref_time usage and limit.
pub struct RefTimeMeter(Resource<u64>);

impl RefTimeMeter {
	/// Creates a new `RefTimeResource` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, ResourceError> {
		Ok(Self(Resource::new(0, limit)?))
	}

	/// Records the ref_time and updates the usage.
	pub fn record_ref_time(&mut self, ref_time: u64) -> Result<(), ResourceError> {
		self.0.record_cost(ref_time)
	}

	/// Returns the ref time usage.
	pub fn usage(&self) -> u64 {
		self.0.usage()
	}

	/// Returns the ref time limit.
	pub fn limit(&self) -> u64 {
		self.0.limit
	}

	/// Refunds the given amount of ref_time.
	pub fn refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}
}

/// A struct that keeps track of storage usage (newly created storage) and limit.
pub struct StorageMeter(Resource<u64>);

impl StorageMeter {
	/// Creates a new `StorageResource` instance with the given limit.
	pub fn new(limit: u64) -> Result<Self, ResourceError> {
		Ok(Self(Resource::new(0, limit)?))
	}

	/// Refunds the given amount of storage.
	fn _refund(&mut self, amount: u64) {
		self.0.refund(amount)
	}

	/// Returns the storage usage.
	pub fn usage(&self) -> u64 {
		self.0.usage()
	}

	/// Records the dynamic opcode cost and updates the storage usage.
	///
	/// # Errors
	///
	/// Returns `ResourceError::LimitExceeded` if the storage usage exceeds the storage limit.
	fn _record_dynamic_opcode_cost(
		&mut self,
		_opcode: Opcode,
		gas_cost: GasCost,
	) -> Result<(), ResourceError> {
		let cost = match gas_cost {
			GasCost::Create => {
				// TODO record cost for create
				0
			}
			GasCost::Create2 { len } => {
				// len in bytes ??
				len.try_into().map_err(|_| ResourceError::LimitExceeded)?
			}
			GasCost::SStore { .. } => {
				// TODO record cost for sstore
				0
			}
			_ => return Ok(()),
		};
		self.0.record_cost(cost)
	}

	fn record_external_operation(
		&mut self,
		operation: &evm::ExternalOperation,
		_contract_size_limit: u64,
	) {
		match operation {
			evm::ExternalOperation::Write => {
				// Todo record cost for write
			}
			_ => {}
		}
	}
}

pub struct ResourceInfo<T> {
	pub ref_time_meter: Option<RefTimeMeter>,
	pub proof_size_meter: Option<ProofSizeMeter<T>>,
	pub storage_meter: Option<StorageMeter>,
}

impl<T: Config> ResourceInfo<T> {
	pub fn new() -> Self {
		Self {
			ref_time_meter: None,
			proof_size_meter: None,
			storage_meter: None,
		}
	}

	pub fn add_ref_time_meter(&mut self, limit: u64) -> Result<(), &'static str> {
		self.ref_time_meter = Some(RefTimeMeter::new(limit).map_err(|_| "Invalid parameters")?);
		Ok(())
	}

	pub fn add_proof_size_meter(&mut self, base_cost: u64, limit: u64) -> Result<(), &'static str> {
		self.proof_size_meter =
			Some(ProofSizeMeter::new(base_cost, limit).map_err(|_| "Invalid parameters")?);
		Ok(())
	}

	pub fn add_storage_meter(&mut self, limit: u64) -> Result<(), &'static str> {
		self.storage_meter = Some(StorageMeter::new(limit).map_err(|_| "Invalid parameters")?);
		Ok(())
	}

	pub fn refund_proof_size(&mut self, amount: u64) {
		self.proof_size_meter.as_mut().map(|proof_size_meter| {
			proof_size_meter.refund(amount);
		});
	}

	pub fn refund_ref_time(&mut self, amount: u64) {
		self.ref_time_meter.as_mut().map(|ref_time_meter| {
			ref_time_meter.refund(amount);
		});
	}

	/// Returns WeightInfo for the resource.
	pub fn weight_info(&self) -> WeightInfo {
		macro_rules! usage_and_limit {
			($x:expr) => {
				(
					$x.as_ref().map(|x| x.usage()),
					$x.as_ref().map(|x| x.limit()),
				)
			};
		}

		let (proof_size_usage, proof_size_limit) = usage_and_limit!(self.proof_size_meter);
		let (ref_time_usage, ref_time_limit) = usage_and_limit!(self.ref_time_meter);

		WeightInfo {
			proof_size_usage,
			proof_size_limit,
			ref_time_usage,
			ref_time_limit,
		}
	}

	pub fn record_external_operation(
		&mut self,
		operation: evm::ExternalOperation,
		contract_size_limit: u64,
	) -> Result<(), ResourceError> {
		if let Some(proof_size_meter) = self.proof_size_meter.as_mut() {
			proof_size_meter.record_external_operation(&operation, contract_size_limit)?;
		}

		if let Some(storage_meter) = self.storage_meter.as_mut() {
			storage_meter.record_external_operation(&operation, contract_size_limit)
		}

		Ok(())
	}

	pub fn record_external_dynamic_opcode_cost(
		&mut self,
		opcode: Opcode,
		target: evm::gasometer::StorageTarget,
		contract_size_limit: u64,
	) -> Result<(), ResourceError> {
		if let Some(proof_size_meter) = self.proof_size_meter.as_mut() {
			proof_size_meter.record_external_dynamic_opcode_cost(
				opcode,
				target,
				contract_size_limit,
			)?;
		}
		// Record ref_time
		// TODO benchmark opcodes, until this is done we do used_gas to weight conversion for ref_time

		Ok(())
	}

	/// Computes the effective gas for the transaction. Effective gas is the maximum between the
	/// gas used and resource usage.
	pub fn effective_gas(&self, gas: u64) -> U256 {
		let proof_size_usage = self
			.proof_size_meter
			.as_ref()
			.map_or(0, |meter| meter.usage())
			.saturating_mul(T::GasLimitPovSizeRatio::get());

		// TODO: use the actual ref time usage
		// let ref_time_usage = self
		// 	.ref_time_meter
		// 	.map_or(0, |meter| meter.usage())
		// 	.saturating_mul(T::GasLimitPovRefTimeRatio::get());

		// TODO get the Storage Gas ratio
		let storage_usage = self.storage_meter.as_ref().map_or(0, |meter| meter.usage());

		let effective_gas =
			sp_std::cmp::max(sp_std::cmp::max(proof_size_usage, storage_usage), gas);

		U256::from(effective_gas)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_init() {
		let resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.limit, 100);
		assert_eq!(resource.usage, 0);

		// base cost > limit
		let resource = Resource::<u64>::new(100, 0).err();
		assert_eq!(resource, Some(ResourceError::InvalidBaseCost));
	}

	#[test]
	fn test_record_cost() {
		let mut resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.record_cost(10), Ok(()));
		assert_eq!(resource.usage, 10);
		assert_eq!(resource.record_cost(90), Ok(()));
		assert_eq!(resource.usage, 100);

		// exceed limit
		assert_eq!(resource.record_cost(1), Err(ResourceError::LimitExceeded));
		assert_eq!(resource.usage, 100);
	}

	#[test]
	fn test_refund() {
		let mut resource = Resource::<u64>::new(0, 100).unwrap();
		assert_eq!(resource.record_cost(10), Ok(()));
		assert_eq!(resource.usage, 10);
		resource.refund(10);
		assert_eq!(resource.usage, 0);

		// refund more than usage
		resource.refund(10);
		assert_eq!(resource.usage, 0);
	}

	#[test]
	fn test_storage_resource() {
		let mut resource = StorageMeter::new(100).unwrap();
		assert_eq!(resource.0.usage, 0);
		assert_eq!(resource.0.limit, 100);
		assert_eq!(resource.0.record_cost(10), Ok(()));
		assert_eq!(resource.0.usage, 10);
		assert_eq!(resource.0.record_cost(90), Ok(()));
		assert_eq!(resource.0.usage, 100);
		assert_eq!(resource.0.record_cost(1), Err(ResourceError::LimitExceeded));
		assert_eq!(resource.0.usage, 100);
		resource.0.refund(10);
		assert_eq!(resource.0.usage, 90);
		resource._refund(10);
		assert_eq!(resource.0.usage, 80);
	}
}
