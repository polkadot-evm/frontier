import { ApiPromise, WsProvider, Keyring } from "@polkadot/api";
import { KeyringPair } from '@polkadot/keyring/types';
import { U8aFixed } from '@polkadot/types/codec';
import * as web3Utils from 'web3-utils';
import * as crypto from '@polkadot/util-crypto';


// Provider is set to localhost for development
const wsProvider = new WsProvider("ws://localhost:9944");

// Keyring needed to sign using Alice account
const keyring = new Keyring({ type: 'sr25519' });

// ByteCode of our ERC20 exemple: copied from ../truffle/contracts/MyToken.json
const ERC20_BYTECODES = "0x608060405234801561001057600080fd5b50610041337fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff61004660201b60201c565b610291565b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff1614156100e9576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601f8152602001807f45524332303a206d696e7420746f20746865207a65726f20616464726573730081525060200191505060405180910390fd5b6101028160025461020960201b610c7c1790919060201c565b60028190555061015d816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205461020960201b610c7c1790919060201c565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff16600073ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a35050565b600080828401905083811015610287576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b8091505092915050565b610e3a806102a06000396000f3fe608060405234801561001057600080fd5b50600436106100885760003560e01c806370a082311161005b57806370a08231146101fd578063a457c2d714610255578063a9059cbb146102bb578063dd62ed3e1461032157610088565b8063095ea7b31461008d57806318160ddd146100f357806323b872dd146101115780633950935114610197575b600080fd5b6100d9600480360360408110156100a357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610399565b604051808215151515815260200191505060405180910390f35b6100fb6103b7565b6040518082815260200191505060405180910390f35b61017d6004803603606081101561012757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803590602001909291905050506103c1565b604051808215151515815260200191505060405180910390f35b6101e3600480360360408110156101ad57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff1690602001909291908035906020019092919050505061049a565b604051808215151515815260200191505060405180910390f35b61023f6004803603602081101561021357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919050505061054d565b6040518082815260200191505060405180910390f35b6102a16004803603604081101561026b57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610595565b604051808215151515815260200191505060405180910390f35b610307600480360360408110156102d157600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610662565b604051808215151515815260200191505060405180910390f35b6103836004803603604081101561033757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190505050610680565b6040518082815260200191505060405180910390f35b60006103ad6103a6610707565b848461070f565b6001905092915050565b6000600254905090565b60006103ce848484610906565b61048f846103da610707565b61048a85604051806060016040528060288152602001610d7060289139600160008b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000206000610440610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b600190509392505050565b60006105436104a7610707565b8461053e85600160006104b8610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008973ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b61070f565b6001905092915050565b60008060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020549050919050565b60006106586105a2610707565b8461065385604051806060016040528060258152602001610de160259139600160006105cc610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008a73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b6001905092915050565b600061067661066f610707565b8484610906565b6001905092915050565b6000600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054905092915050565b600033905090565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff161415610795576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526024815260200180610dbd6024913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff16141561081b576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526022815260200180610d286022913960400191505060405180910390fd5b80600160008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925836040518082815260200191505060405180910390a3505050565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff16141561098c576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526025815260200180610d986025913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff161415610a12576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526023815260200180610d056023913960400191505060405180910390fd5b610a7d81604051806060016040528060268152602001610d4a602691396000808773ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b6000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002081905550610b10816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a3505050565b6000838311158290610c69576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825283818151815260200191508051906020019080838360005b83811015610c2e578082015181840152602081019050610c13565b50505050905090810190601f168015610c5b5780820380516001836020036101000a031916815260200191505b509250505060405180910390fd5b5060008385039050809150509392505050565b600080828401905083811015610cfa576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b809150509291505056fe45524332303a207472616e7366657220746f20746865207a65726f206164647265737345524332303a20617070726f766520746f20746865207a65726f206164647265737345524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e636545524332303a207472616e7366657220616d6f756e74206578636565647320616c6c6f77616e636545524332303a207472616e736665722066726f6d20746865207a65726f206164647265737345524332303a20617070726f76652066726f6d20746865207a65726f206164647265737345524332303a2064656372656173656420616c6c6f77616e63652062656c6f77207a65726fa265627a7a72315820c7a5ffabf642bda14700b2de42f8c57b36621af020441df825de45fd2b3e1c5c64736f6c63430005100032";

// Setup the API and Alice Account
async function init() {
    console.log(`Initiating the API (ignore message "Unable to resolve type B..." and "Unknown types found...")`);

    // Initiate the polkadot API.
    const api = await ApiPromise.create({
        provider: wsProvider,
        types: {
            // mapping the actual specified address format
            Address: "AccountId",
            // mapping the lookup
            LookupSource: "AccountId",
            Account: {
                nonce: "U256",
                balance: "U256"
            }
        }
    });
    console.log(`Initialiation done`);
    console.log(`Genesis at block: ${api.genesisHash.toHex()}`);

    const alice = keyring.addFromUri('//Alice', { name: 'Alice default' });
    const bob = keyring.addFromUri('//Bob', { name: 'Bob default' });

    const { nonce, data: balance } = await api.query.system.account(alice.address);
    console.log(`Alice Substrate Account: ${alice.address}`);
    console.log(`Alice Substrate Account (nonce: ${nonce}) balance, free: ${balance.free.toHex()}`);

    const aliceEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(alice.address), 256).substring(26)}`;

    console.log(`Alice EVM Account: ${aliceEvmAccount}`);
    const evmData = (await api.query.evm.accounts(aliceEvmAccount)) as any;
    console.log(`Alice EVM Account (nonce: ${evmData.nonce}) balance: ${evmData.balance.toHex()}`);

    0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d

    return { api, alice, bob };
}

// Create the ERC20 contract from ALICE
async function step1(api: ApiPromise, alice: KeyringPair) {

    console.log(`\nStep 1: Creating Smart Contract`);

    // params: [bytecode, initialBalance, gasLimit, gasPrice],
    // tx: api.tx.evm.create

    const transaction = await api.tx.evm.create(ERC20_BYTECODES, 0, 4294967295, 1, null);

    const contract = new Promise<{ block: string, address: string }>(async (resolve, reject) => {
        const unsub = await transaction.signAndSend(alice, (result) => {
            console.log(`Contract creation is ${result.status}`);
            if (result.status.isInBlock) {
                console.log(`Contract included at blockHash ${result.status.asInBlock}`);
                console.log(`Waiting for finalization... (can take a minute)`);
            } else if (result.status.isFinalized) {
                const contractAddress = (
                    result.events?.find(
                        event => event?.event?.index.toHex() == "0x0500"
                    )?.event.data[0] as any
                ).address as string;
                console.log(`Contract finalized at blockHash ${result.status.asFinalized}`);
                console.log(`Contract address: ${contractAddress}`);
                unsub();
                resolve({
                    block: result.status.asFinalized.toString(),
                    address: contractAddress
                });
            }
        });
    });
    return contract;
}

// Retrieve Alice & Contract Storage
async function step2(api: ApiPromise, alice: KeyringPair, contractAddress: string) {

    console.log(`\nStep 2: Retrieving Contract from evm address: ${contractAddress}`);

    // Retrieve Alice account with new nonce value
    const { nonce, data: balance } = await api.query.system.account(alice.address);
    console.log(`Alice Substrate Account (nonce: ${nonce}) balance, free: ${balance.free}`);

    const accountCode = (await api.query.evm.accountCodes(contractAddress)).toString();
    console.log(`Contract account code: ${accountCode.substring(0, 16)}...${accountCode.substring(accountCode.length - 16)}`);

    // Computing Contract Storage Slot, using slot 0 and alice EVM account
    const aliceEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(alice.address), 256).substring(26)}`;
    const slot = "0";
    const mapStorageSlot = slot.padStart(64, '0');
    const mapKey = aliceEvmAccount.toString().substring(2).padStart(64, '0');

    const storageKey = web3Utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
    console.log(`Alice Contract storage key: ${storageKey}`);

    const accountStorage = (await api.query.evm.accountStorages(contractAddress, storageKey)).toString();
    console.log(`Alice Contract account storage: ${accountStorage}`);
    return;
}


// Transfer tokens to Bob
async function step3(api: ApiPromise, alice: KeyringPair, bob: KeyringPair, contractAddress: string) {

    const bobEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(bob.address), 256).substring(26)}`;
    console.log(`\nStep 3: Transfering Tokens to Bob EVM Account: ${bobEvmAccount}`);

    console.log(`Preparing transfer of 0xdd`);
    // params: [contractAddress, inputCode, value,m gasLimit, gasPrice],
    // tx: api.tx.evm.create
    const transferFnCode = `a9059cbb000000000000000000000000`;
    const tokensToTransfer = `00000000000000000000000000000000000000000000000000000000000000dd`;
    const inputCode = `0x${transferFnCode}${bobEvmAccount.substring(2)}${tokensToTransfer}`;
    console.log(`Sending call input: ${inputCode}`);
    const transaction = await api.tx.evm.call(contractAddress, inputCode, 0, 4294967295, 1, null);

    const data = new Promise<{ block: string, address: string }>(async (resolve, reject) => {
        const unsub = await transaction.signAndSend(alice, (result) => {
            console.log(`Transfer is ${result.status}`);
            if (result.status.isInBlock) {
                console.log(`Transfer included at blockHash ${result.status.asInBlock}`);
                console.log(`Waiting for finalization... (can take a minute)`);
            } else if (result.status.isFinalized) {
                console.log(`Transfer finalized at blockHash ${result.status.asFinalized}`);
                unsub();
                resolve();
            }
        });
    });
    return data;
}

// Retrieve Bob
async function step4(api: ApiPromise, bob: KeyringPair, contractAddress: string) {

    console.log(`\nStep 4: Retrieving Bob tokens`);

    // Retrieve Bob account with new nonce value
    const { nonce, data: balance } = await api.query.system.account(bob.address);
    console.log(`Bob Substrate Account (nonce: ${nonce}) balance, free: ${balance.free}`);
    const bobEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(bob.address), 256).substring(26)}`;

    console.log(`Bob EVM Account: ${bobEvmAccount}`);
    const evmData = (await api.query.evm.accounts(bobEvmAccount)) as any;
    console.log(`Bob EVM Account (nonce: ${evmData.nonce}) balance: ${evmData.balance.toHex()}`);

    const slot = "0";
    const mapStorageSlot = slot.padStart(64, '0');
    const mapKey = bobEvmAccount.toString().substring(2).padStart(64, '0');

    const storageKey = web3Utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
    console.log(`Bob Contract storage key: ${storageKey}`);

    const accountStorage = (await api.query.evm.accountStorages(contractAddress, storageKey)).toString();
    console.log(`Bob Contract account storage: ${accountStorage}`);

    return;
}

async function main() {
    const { api, alice, bob } = await init();

    // step 1: Creating the contract from ALICE
    const contractAccount = await step1(api, alice)

    // step 2: Retrieving Alice and Contract information
    await step2(api, alice, contractAccount.address);

    // step 3: Transfering Smart Contract tokens from Alice to Bob
    await step3(api, alice, bob, contractAccount.address);

    // step 3: Retrieving Bob information
    await step4(api, bob, contractAccount.address);
}

main().catch(console.error).then(() => process.exit(0));
