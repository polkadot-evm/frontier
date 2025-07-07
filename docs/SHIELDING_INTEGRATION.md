# Shielding Pallet Integration Guide

This guide explains how to use the shielding pallet to store notes from EVM shield operations.

## Overview

The shielding pallet has been integrated with the EVM to automatically store notes from shield operations in a Merkle tree. When an EVM contract calls the `shield` function, the note is automatically added to the shielding pallet's Merkle tree.

## Architecture

### Components

1. **EVM Shield Function**: Transfers funds and generates note hashes
2. **OnShield Hook**: Integrates EVM with the shielding pallet
3. **Shielding Pallet**: Stores notes in a Merkle tree
4. **Merkle Tree**: Provides efficient inclusion proofs

### Flow

```
EVM Contract → shield() → OnShield Hook → Shielding Pallet → Merkle Tree
```

## Configuration

### Runtime Configuration

The integration is configured in `frontier/template/runtime/src/lib.rs`:

```rust
// EVM configuration
impl pallet_evm::Config for Runtime {
    // ... other config ...
    type OnShield = ShieldingHook;
}

// Shielding hook implementation
pub struct ShieldingHook;

impl pallet_evm::OnShield<Runtime> for ShieldingHook {
    fn on_shield(_source: H160, _value: U256, note: H256) -> Result<(), DispatchError> {
        // Add the note to the shielding pallet's Merkle tree
        shielding::Pallet::<Runtime>::add_note(
            frame_system::RawOrigin::None.into(),
            note,
        )
    }
}

// Shielding pallet configuration
impl shielding::Config for Runtime {
    type MaxTreeDepth = ConstU32<20>; // 2^20 = 1,048,576 notes
    type RuntimeEvent = RuntimeEvent;
}
```

## Usage

### 1. From EVM Contracts

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ShieldingExample {
    address public constant SHIELDING_POOL = address(0x0000000000000000000000000000000000000000);
    
    event FundsShielded(address indexed sender, uint256 amount, bytes32 noteHash);
    
    function shieldFunds(uint256 amount, bytes32 noteHash) external payable {
        require(msg.value == amount, "Incorrect amount sent");
        
        // Call the shield function - this automatically integrates with the shielding pallet
        (bool success, ) = SHIELDING_POOL.call{value: amount}(
            abi.encodeWithSignature("shield(address,uint256,bytes32)", msg.sender, amount, noteHash)
        );
        
        require(success, "Shield operation failed");
        
        emit FundsShielded(msg.sender, amount, noteHash);
    }
}
```

### 2. From Substrate/Polkadot.js Apps

```javascript
// Add a note directly to the shielding pallet
const noteHash = "0x1234567890abcdef..."; // 32-byte hash
await api.tx.shielding.addNote(noteHash).signAndSend(alice);

// Query the Merkle root
const merkleRoot = await api.query.shielding.merkleRoot();

// Query note count
const noteCount = await api.query.shielding.noteCount();

// Query a specific note
const note = await api.query.shielding.notes(0);
```

### 3. From Rust Code

```rust
use frame_system::RawOrigin;
use sp_core::H256;

// Add a note to the Merkle tree
let note_hash = H256::from_slice(&[1u8; 32]);
let _ = Shielding::add_note(RawOrigin::Signed(alice).into(), note_hash);

// Get the current Merkle root
let root = Shielding::merkle_root();

// Get the note count
let count = Shielding::note_count();
```

## API Reference

### Shielding Pallet Extrinsics

- **`add_note(note: H256)`**: Add a note to the Merkle tree
- **`shield_funds(amount, recipient)`**: Shield funds (if implemented)
- **`unshield_funds(amount, proof, nullifier)`**: Unshield funds (if implemented)

### Shielding Pallet Queries

- **`merkleRoot()`**: Get the current Merkle root
- **`noteCount()`**: Get the total number of notes
- **`notes(index: u64)`**: Get a specific note by index

### Events

- **`NoteAdded { note: H256, index: u64, root: H256 }`**: Emitted when a note is added
- **`MerkleRootUpdated { new_root: H256 }`**: Emitted when the Merkle root is updated

## Testing

### Run the Example

```bash
# Start the node
cargo run --bin frontier-template-node -- --dev

# In another terminal, run the integration example
node frontier/examples/shielding-integration-example.js
```

### Expected Output

```
🚀 Demonstrating Shielding Integration...

📊 Initial State:
Initial Merkle root: 0x0000000000000000000000000000000000000000000000000000000000000000
Initial note count: 0

📝 Test note hash: 0x1234567890abcdef...

🛡️  Simulating EVM shield operation...
✅ Shield transaction included in block
📋 Shielding event: NoteAdded
   - Note: 0x1234567890abcdef...
   - Index: 0
   - New root: 0xabcdef1234567890...

📊 Updated State:
New Merkle root: 0xabcdef1234567890...
New note count: 1
Root changed: true
Count increased: true

🔍 Verifying note storage:
✅ Note found at index 0: 0x1234567890abcdef...
   Matches our note: true
```

## Security Considerations

### Privacy Properties

- **Note Privacy**: Individual notes are stored as hashes
- **Merkle Tree**: Efficient inclusion proofs without revealing all data
- **Zero-Knowledge**: Future implementations can add ZK proof verification

### Limitations

- **Current Implementation**: Basic Merkle tree without ZK proofs
- **Note Size**: Limited by the Merkle tree depth (2^20 notes)
- **Gas Costs**: Shield operations require gas for both EVM and Substrate operations

## Future Enhancements

### Planned Features

1. **Zero-Knowledge Proofs**: Add ZK proof verification for unshielding
2. **Nullifier System**: Prevent double-spending of shielded notes
3. **Note Encryption**: Encrypt note data for additional privacy
4. **Batch Operations**: Support for batch shielding/unshielding
5. **Cross-Chain**: Support for cross-chain shielded transfers

### Integration Points

- **Precompiles**: Add shielding precompiles for easier EVM integration
- **RPC**: Add RPC methods for querying shielding state
- **Frontend**: Add UI components for shielding operations

## Troubleshooting

### Common Issues

1. **Note Not Added**: Check if the OnShield hook is properly configured
2. **Merkle Root Not Updated**: Verify the shielding pallet is working correctly
3. **Gas Limit Exceeded**: Increase gas limit for shield operations

### Debugging

```bash
# Check shielding pallet logs
cargo run --bin frontier-template-node -- --dev -l shielding=debug

# Query shielding state
curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_call", "params": ["Shielding_merkle_root", "0x"]}' http://localhost:9933
```

## Conclusion

The shielding pallet integration provides a seamless way to store notes from EVM shield operations in a Substrate-based Merkle tree. This enables privacy-preserving transactions while maintaining compatibility with existing EVM infrastructure.

For more information, see the [shielding pool documentation](./SHIELDING_POOL.md) and the [example implementation](../examples/shielding-integration-example.js). 