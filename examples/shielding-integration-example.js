// Example: Using the shielding pallet with EVM shield function
// This demonstrates how notes from EVM shield operations are stored in the shielding pallet

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { ethers } = require('ethers');

async function demonstrateShieldingIntegration() {
    console.log('🚀 Demonstrating Shielding Integration...\n');

    // Connect to the node
    const wsProvider = new WsProvider('ws://localhost:9944');
    const api = await ApiPromise.create({ provider: wsProvider });

    // Create a keyring for signing transactions
    const keyring = new Keyring({ type: 'sr25519' });
    const alice = keyring.addFromUri('//Alice');

    try {
        // 1. Get initial state
        console.log('📊 Initial State:');
        const initialRoot = await api.query.shielding.merkleRoot();
        const initialCount = await api.query.shielding.noteCount();
        console.log(`Initial Merkle root: ${initialRoot.toString()}`);
        console.log(`Initial note count: ${initialCount.toString()}\n`);

        // 2. Create a test note hash (in a real scenario, this would be computed from note data)
        const noteData = ethers.utils.toUtf8Bytes('Test shielded note data');
        const noteHash = ethers.utils.keccak256(noteData);
        console.log(`📝 Test note hash: ${noteHash}\n`);

        // 3. Simulate an EVM shield operation
        // In a real scenario, this would be called from an EVM contract
        console.log('🛡️  Simulating EVM shield operation...');
        
        // Create a transaction that would trigger the shield function
        // This is a simplified example - in practice, this would be called from EVM
        const shieldTx = api.tx.shielding.addNote(noteHash);
        
        const hash = await shieldTx.signAndSend(alice, ({ events, status }) => {
            if (status.isInBlock) {
                console.log('✅ Shield transaction included in block');
                events.forEach(({ event }) => {
                    if (event.section === 'shielding') {
                        console.log(`📋 Shielding event: ${event.method}`);
                        if (event.method === 'NoteAdded') {
                            const [note, index, root] = event.data;
                            console.log(`   - Note: ${note.toString()}`);
                            console.log(`   - Index: ${index.toString()}`);
                            console.log(`   - New root: ${root.toString()}`);
                        }
                    }
                });
            }
        });

        console.log(`Transaction hash: ${hash.toString()}\n`);

        // 4. Wait a moment for the transaction to be processed
        await new Promise(resolve => setTimeout(resolve, 2000));

        // 5. Check the updated state
        console.log('📊 Updated State:');
        const newRoot = await api.query.shielding.merkleRoot();
        const newCount = await api.query.shielding.noteCount();
        console.log(`New Merkle root: ${newRoot.toString()}`);
        console.log(`New note count: ${newCount.toString()}`);
        console.log(`Root changed: ${initialRoot.toString() !== newRoot.toString()}`);
        console.log(`Count increased: ${newCount.toNumber() > initialCount.toNumber()}\n`);

        // 6. Verify the note was stored
        console.log('🔍 Verifying note storage:');
        const storedNote = await api.query.shielding.notes(0); // Get the first note
        if (storedNote.isSome) {
            console.log(`✅ Note found at index 0: ${storedNote.unwrap().toString()}`);
            console.log(`   Matches our note: ${storedNote.unwrap().toString() === noteHash}`);
        } else {
            console.log('❌ Note not found');
        }

        // 7. Demonstrate how this integrates with EVM
        console.log('\n🔗 EVM Integration:');
        console.log('In a real EVM contract, the shield function would:');
        console.log('1. Transfer funds to the shielding pool');
        console.log('2. Generate a note hash from the transaction data');
        console.log('3. Call the OnShield hook automatically');
        console.log('4. Store the note in the shielding pallet\'s Merkle tree');
        console.log('5. Update the Merkle root');
        console.log('\nThis integration ensures that all EVM shield operations');
        console.log('are properly recorded in the Substrate shielding pallet!');

    } catch (error) {
        console.error('❌ Error:', error);
    } finally {
        await api.disconnect();
    }
}

// Example EVM contract that would use the shield function
const exampleContract = `
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ShieldingExample {
    address public constant SHIELDING_POOL = address(0x0000000000000000000000000000000000000000);
    
    event FundsShielded(address indexed sender, uint256 amount, bytes32 noteHash);
    
    function shieldFunds(uint256 amount, bytes32 noteHash) external payable {
        require(msg.value == amount, "Incorrect amount sent");
        
        // In the EVM implementation, this would call the shield function
        // which automatically integrates with the Substrate shielding pallet
        (bool success, ) = SHIELDING_POOL.call{value: amount}(
            abi.encodeWithSignature("shield(address,uint256,bytes32)", msg.sender, amount, noteHash)
        );
        
        require(success, "Shield operation failed");
        
        emit FundsShielded(msg.sender, amount, noteHash);
    }
}
`;

console.log('📋 Example EVM Contract:');
console.log(exampleContract);

// Run the demonstration
if (require.main === module) {
    demonstrateShieldingIntegration().catch(console.error);
}

module.exports = { demonstrateShieldingIntegration }; 