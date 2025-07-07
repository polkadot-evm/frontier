# Merkle Tree Shielding Pool

This document describes the Merkle tree shielding pool implementation for Frontier, which provides Zcash-like privacy features for EVM-compatible blockchains.

## Overview

The shielding pool allows users to perform privacy-preserving transactions by:

1. **Shielding**: Converting transparent funds into shielded commitments
2. **Transferring**: Moving shielded funds between accounts without revealing amounts or recipients
3. **Unshielding**: Converting shielded funds back to transparent balances

The implementation uses:
- **Merkle Trees**: For efficient commitment storage and verification
- **Zero-Knowledge Proofs**: For transaction validity without revealing details
- **Nullifiers**: To prevent double-spending of shielded notes
- **Note Encryption**: To protect transaction privacy

## Architecture

### Components

1. **Substrate Pallet** (`frame-shielding`): Core shielding pool logic
2. **EVM Precompile** (`frontier-precompiles`): Smart contract interface
3. **Solidity Interface**: Type-safe smart contract interactions
4. **Runtime Integration**: Configuration and setup

### Key Features

- **Privacy**: Transaction amounts and recipients are hidden
- **Efficiency**: Merkle tree provides O(log n) proof generation
- **Security**: Cryptographic proofs ensure transaction validity
- **Compatibility**: Works with existing EVM smart contracts

## Usage

### From Smart Contracts

#### Basic Shielding

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

import "./ShieldingPool.sol";

contract MyShieldingContract {
    using ShieldingPoolLibrary for address;
    
    address constant SHIELDING_POOL = address(0x0000000000000000000000000000000000000010);
    
    function shieldFunds(address recipient, uint256 amount) external {
        (bool success, bytes32 commitment) = SHIELDING_POOL.shield(recipient, amount);
        require(success, "Shield operation failed");
        
        emit FundsShielded(msg.sender, amount, commitment);
    }
    
    function unshieldFunds(uint256 amount, bytes memory proof, bytes32 nullifier) external {
        bool success = SHIELDING_POOL.unshield(amount, proof, nullifier);
        require(success, "Unshield operation failed");
        
        emit FundsUnshielded(msg.sender, amount, nullifier);
    }
    
    function getMerkleRoot() external view returns (bytes32) {
        return SHIELDING_POOL.getMerkleRoot();
    }
    
    function getShieldedBalance(address account) external view returns (uint256) {
        return SHIELDING_POOL.getShieldedBalance(account);
    }
}
```

#### Advanced Usage with Custom Logic

```solidity
contract AdvancedShielding {
    using ShieldingPoolLibrary for address;
    
    mapping(address => bytes32[]) public userCommitments;
    mapping(address => uint256) public userShieldedBalances;
    
    function shieldWithTracking(address recipient, uint256 amount) external {
        (bool success, bytes32 commitment) = address(0x0000000000000000000000000000000000000010).shield(recipient, amount);
        require(success, "Shield failed");
        
        userCommitments[recipient].push(commitment);
        userShieldedBalances[recipient] += amount;
    }
    
    function batchShield(address[] calldata recipients, uint256[] calldata amounts) external {
        require(recipients.length == amounts.length, "Arrays must match");
        
        for (uint i = 0; i < recipients.length; i++) {
            (bool success, ) = address(0x0000000000000000000000000000000000000010).shield(recipients[i], amounts[i]);
            require(success, "Batch shield failed");
        }
    }
    
    function getCommitmentCount() external view returns (uint256) {
        return address(0x0000000000000000000000000000000000000010).getCommitmentCount();
    }
}
```

### From Substrate Runtime

#### Direct Pallet Calls

```rust
use frame_shielding as shielding;
use sp_core::H256;

// Shield funds
let amount = 1000u128;
let recipient = account_id;
let call = shielding::Call::<Runtime>::shield_funds { amount, recipient };
let origin = RuntimeOrigin::from(Some(caller));
call.dispatch(origin)?;

// Unshield funds
let proof = shielding::ShieldProof::new(
    amount.into(),
    recipient.encode(),
    nullifier,
    commitment,
);
let call = shielding::Call::<Runtime>::unshield_funds { amount, proof, nullifier };
call.dispatch(origin)?;
```

#### Using Helper Functions

```rust
use crate::shielding::helpers;

// Create a commitment
let commitment = helpers::create_commitment(amount, recipient);

// Create a nullifier
let nullifier = helpers::create_nullifier(commitment_hash, spending_key);

// Create a shield proof
let proof = helpers::create_shield_proof(amount, recipient, nullifier, commitment);

// Get current state
let merkle_root = helpers::get_merkle_root();
let commitment_count = helpers::get_commitment_count();
let shielded_balance = helpers::get_shielded_balance(account);
```

## Configuration

### Runtime Parameters

```rust
parameter_types! {
    pub const MaxCommitments: u32 = 1_000_000; // 1 million commitments
    pub const MaxNullifiers: u32 = 1_000_000;  // 1 million nullifiers
    pub const ShieldDeposit: Balance = 1_000_000_000_000_000; // 0.001 tokens
    pub const MinShieldAmount: Balance = 1; // 1 wei minimum
    pub const MaxShieldAmount: Balance = 1_000_000_000_000_000_000_000_000; // 1 billion tokens max
}
```

### Pallet Configuration

```rust
impl shielding::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = pallet_balances::Pallet<Runtime>;
    type MaxCommitments = MaxCommitments;
    type MaxNullifiers = MaxNullifiers;
    type ShieldDeposit = ShieldDeposit;
    type MinShieldAmount = MinShieldAmount;
    type MaxShieldAmount = MaxShieldAmount;
    type WeightInfo = shielding::weights::SubstrateWeight<Runtime>;
}
```

### Precompile Configuration

```rust
impl pallet_evm::Config for Runtime {
    // ... existing config ...
    type PrecompilesType = ShieldingPrecompiles<Runtime>;
    type PrecompilesValue = ShieldingPrecompiles<Runtime>;
}
```

## Security Considerations

### Cryptographic Assumptions

- **Blake2b**: Used for hashing commitments and nullifiers
- **Merkle Tree**: Provides efficient inclusion proofs
- **Zero-Knowledge Proofs**: Ensure transaction validity without revealing details

### Privacy Properties

- **Amount Privacy**: Transaction amounts are hidden
- **Recipient Privacy**: Recipients are not publicly visible
- **Sender Privacy**: Senders are not linked to transactions
- **Balance Privacy**: Account balances are not revealed

### Attack Vectors

1. **Double-Spending**: Prevented by nullifiers
2. **Replay Attacks**: Prevented by unique nullifiers
3. **Front-Running**: Mitigated by commitment schemes
4. **Sybil Attacks**: Limited by economic constraints

## Gas Costs

| Operation | Base Cost | Per Byte Cost | Description |
|-----------|-----------|---------------|-------------|
| Shield | 50,000 | 10 | Create commitment and add to Merkle tree |
| Unshield | 60,000 | 10 | Verify proof and transfer funds |
| Transfer | 70,000 | 10 | Transfer between shielded accounts |
| GetMerkleRoot | 2,000 | 0 | Read current Merkle root |
| GetCommitmentCount | 2,000 | 0 | Read commitment count |
| GetCommitment | 2,000 | 10 | Read commitment by index |
| IsNullifierUsed | 2,000 | 10 | Check nullifier status |
| GetShieldedBalance | 2,000 | 10 | Read shielded balance |

## Testing

### Unit Tests

```bash
# Run pallet tests
cargo test -p frame-shielding

# Run precompile tests
cargo test -p frontier-precompiles

# Run integration tests
cargo test -p frontier-template-runtime
```

### Integration Tests

```rust
#[test]
fn test_shielding_workflow() {
    // 1. Shield funds
    let amount = 1000u128;
    let recipient = account_id;
    assert_ok!(ShieldingPallet::shield_funds(Origin::signed(caller), amount, recipient));
    
    // 2. Verify commitment was added
    let merkle_root = ShieldingPallet::merkle_root();
    assert_ne!(merkle_root, H256::zero());
    
    // 3. Check shielded balance
    let balance = ShieldingPallet::shielded_balances(recipient);
    assert_eq!(balance, amount);
    
    // 4. Unshield funds
    let proof = create_test_proof(amount, recipient);
    let nullifier = create_test_nullifier();
    assert_ok!(ShieldingPallet::unshield_funds(Origin::signed(recipient), amount, proof, nullifier));
}
```

### Smart Contract Tests

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

import "./ShieldingPool.sol";
import "@openzeppelin/contracts/test/Test.sol";

contract ShieldingPoolTest is Test {
    ShieldingPoolContract public shieldingContract;
    
    function setUp() public {
        shieldingContract = new ShieldingPoolContract();
    }
    
    function testShieldFunds() public {
        address recipient = address(0x123);
        uint256 amount = 1000;
        
        shieldingContract.shieldFunds(recipient, amount);
        
        bytes32 merkleRoot = shieldingContract.getCurrentMerkleRoot();
        assertTrue(merkleRoot != bytes32(0));
    }
    
    function testUnshieldFunds() public {
        // Test unshielding with valid proof
        uint256 amount = 1000;
        bytes memory proof = new bytes(32);
        bytes32 nullifier = bytes32(uint256(1));
        
        shieldingContract.unshieldFunds(amount, proof, nullifier);
        
        assertTrue(shieldingContract.checkNullifier(nullifier));
    }
}
```

## Deployment

### 1. Add Dependencies

```toml
# Cargo.toml
[dependencies]
frame-shielding = { path = "../../frame/shielding" }
frontier-precompiles = { path = "../precompiles" }
```

### 2. Configure Runtime

```rust
// runtime/src/lib.rs
pub mod shielding;

// Add pallet to construct_runtime!
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = opaque::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        // ... other pallets ...
        Shielding: frame_shielding = 42,
    }
);
```

### 3. Initialize Genesis

```rust
// runtime/src/lib.rs
impl frame_shielding::GenesisConfig<Runtime> {
    pub fn build(&self) {
        // Initialize with empty Merkle tree
    }
}
```

### 4. Deploy Smart Contracts

```bash
# Compile Solidity contracts
npx hardhat compile

# Deploy to network
npx hardhat run scripts/deploy.js --network localhost
```

## Monitoring

### Events

The shielding pool emits the following events:

- `FundsShielded`: When funds are shielded
- `FundsUnshielded`: When funds are unshielded
- `CommitmentAdded`: When a commitment is added to the Merkle tree
- `NullifierUsed`: When a nullifier is used
- `MerkleRootUpdated`: When the Merkle root is updated

### Metrics

Key metrics to monitor:

- **Commitment Count**: Total number of commitments in the Merkle tree
- **Nullifier Count**: Total number of used nullifiers
- **Shielded Balances**: Total value in shielded form
- **Gas Usage**: Gas consumption for shielding operations
- **Proof Verification Time**: Time to verify zero-knowledge proofs

## Troubleshooting

### Common Issues

1. **Insufficient Balance**: Ensure account has enough funds to shield
2. **Invalid Proof**: Check that zero-knowledge proof is correctly generated
3. **Nullifier Already Used**: Each nullifier can only be used once
4. **Merkle Tree Full**: Increase MaxCommitments parameter if needed

### Debug Commands

```bash
# Check pallet state
substrate-node query Shielding merkleRoot
substrate-node query Shielding commitmentCount
substrate-node query Shielding shieldedBalances <account>

# Check precompile
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_call","params":[{"to":"0x0000000000000000000000000000000000000010","data":"0x23456789"}],"id":1}' \
  http://localhost:9933
```

## Future Enhancements

1. **Optimistic Updates**: Reduce proof verification time
2. **Batch Operations**: Support for batch shielding/unshielding
3. **Cross-Chain**: Enable shielded transfers between chains
4. **Advanced Privacy**: Support for confidential amounts and recipients
5. **Gas Optimization**: Reduce gas costs for common operations

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This project is licensed under the Apache 2.0 License - see the LICENSE file for details. 