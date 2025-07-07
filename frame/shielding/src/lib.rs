#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use sp_io::hashing::blake2_256;
	use sp_std::vec::Vec;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The maximum depth of the Merkle tree
		#[pallet::constant]
		type MaxTreeDepth: Get<u32>;
		
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	// Merkle root storage
	#[pallet::storage]
	#[pallet::getter(fn merkle_root)]
	pub type MerkleRoot<T> = StorageValue<_, H256, ValueQuery>;

	// Note count
	#[pallet::storage]
	#[pallet::getter(fn note_count)]
	pub type NoteCount<T> = StorageValue<_, u64, ValueQuery>;

	// Notes (leaves)
	#[pallet::storage]
	#[pallet::getter(fn notes)]
	pub type Notes<T> = StorageMap<_, Blake2_128Concat, u64, H256, OptionQuery>;

	// Internal nodes of the Merkle tree for efficient updates
	#[pallet::storage]
	pub type MerkleNodes<T> = StorageMap<_, Blake2_128Concat, u64, H256, OptionQuery>;


	#[pallet::error]
	pub enum Error<T> {
		/// Merkle tree is full
		MerkleTreeFull,
		/// Invalid tree state
		InvalidTreeState,
		/// Nullifier already used
		NullifierAlreadyUsed,
		/// Invalid note
		InvalidNote,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new note was added to the Merkle tree
		NoteAdded { note: H256, index: u64, root: H256 },
		/// The Merkle root was updated
		MerkleRootUpdated { new_root: H256 },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a note to the Merkle tree (requires signed origin)
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		#[pallet::call_index(0)]
		pub fn add_note(origin: OriginFor<T>, note: H256) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			
			Self::add_note_internal(note)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Add a note to the Merkle tree (internal function, no origin required)
		/// This is used by the OnShield hook which doesn't have a signed origin
		pub fn add_note_internal(note: H256) -> DispatchResult {
			// Check if note is valid (not zero)
			ensure!(!note.is_zero(), Error::<T>::InvalidNote);
			
			// Get current count
			let count = NoteCount::<T>::get();
			let max_leaves = 1 << T::MaxTreeDepth::get();
			
			// Check if tree is full
			ensure!(count < max_leaves as u64, Error::<T>::MerkleTreeFull);
			
			// Add note
			Notes::<T>::insert(count, note);
			
			// Update Merkle tree
			let new_root = Self::update_merkle_tree(count, note)?;
			
			// Update storage
			NoteCount::<T>::put(count + 1);
			MerkleRoot::<T>::put(new_root);
			
			// Emit event
			Self::deposit_event(Event::NoteAdded {
				note,
				index: count,
				root: new_root,
			});
			
			Ok(())
		}

		/// Update the Merkle tree by adding a new leaf
		fn update_merkle_tree(leaf_index: u64, leaf_hash: H256) -> Result<H256, DispatchError> {
			let depth = T::MaxTreeDepth::get();
			let mut current_hash = leaf_hash;
			let mut current_index = leaf_index;
			
			// Store the leaf
			MerkleNodes::<T>::insert(Self::leaf_to_node_index(current_index, depth), current_hash);
			
			// Update the tree bottom-up
			for _level in 0..depth {
				let parent_index = current_index / 2;
				let sibling_index = if current_index % 2 == 0 {
					current_index + 1
				} else {
					current_index - 1
				};
				
				// Get sibling hash (or zero if it doesn't exist)
				let sibling_hash = MerkleNodes::<T>::get(Self::leaf_to_node_index(sibling_index, depth))
					.unwrap_or(H256::zero());
				
				// Compute parent hash
				let parent_hash = if current_index % 2 == 0 {
					Self::hash_pair(current_hash, sibling_hash)
				} else {
					Self::hash_pair(sibling_hash, current_hash)
				};
				
				// Store parent
				MerkleNodes::<T>::insert(Self::leaf_to_node_index(parent_index, depth), parent_hash);
				
				current_hash = parent_hash;
				current_index = parent_index;
			}
			
			Ok(current_hash)
		}

		/// Convert leaf index to node index in the tree
		fn leaf_to_node_index(leaf_index: u64, depth: u32) -> u64 {
			let leaf_start = 1 << depth;
			leaf_start + leaf_index
		}

		/// Hash a pair of hashes
		fn hash_pair(left: H256, right: H256) -> H256 {
			let mut data = Vec::new();
			data.extend_from_slice(left.as_bytes());
			data.extend_from_slice(right.as_bytes());
			blake2_256(&data).into()
		}
	}
}

