import { ethers } from "ethers";
import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, CHAIN_ID, FIRST_CONTRACT_ADDRESS } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

// Simple contract creation bytecode
const SIMPLE_CONTRACT_CREATION = "69602a60005260206000f3600052600a6016f3";

// EIP-7702 delegation prefix
const EIP7702_DELEGATION_PREFIX = "0xef0100";

// Helper function to check if code is a delegation indicator
function isDelegationIndicator(code: string): { isDelegation: boolean; address?: string } {
	if (code && code.length === 48 && code.startsWith(EIP7702_DELEGATION_PREFIX)) {
		const address = "0x" + code.slice(8); // Remove 0xef0100 prefix
		return { isDelegation: true, address };
	}
	return { isDelegation: false };
}

// We use ethers library for EIP-7702 transaction support
describeWithFrontier("Frontier RPC (EIP-7702 Set Code Authorization)", (context: any) => {
	let contractAddress: string;
	let signer: ethers.Wallet;

	// Deploy a test contract first
	step("should deploy delegate test contract", async () => {
		signer = new ethers.Wallet(GENESIS_ACCOUNT_PRIVATE_KEY, context.ethersjs);

		const tx = await signer.sendTransaction({
			data: "0x" + SIMPLE_CONTRACT_CREATION,
			gasLimit: "0x100000",
			gasPrice: "0x3B9ACA00",
		});

		await createAndFinalizeBlock(context.web3);
		const receipt = await context.ethersjs.getTransactionReceipt(tx.hash);

		// Add detailed validation
		contractAddress = receipt.contractAddress;

		if (!contractAddress) {
			throw new Error("Contract deployment failed: contractAddress is null or undefined");
		}

		expect(contractAddress).to.not.be.null;
		expect(contractAddress).to.not.be.undefined;
		expect(contractAddress).to.be.a("string");
		expect(contractAddress).to.match(/^0x[a-fA-F0-9]{40}$/);

		// Verify contract is deployed
		const code = await context.web3.eth.getCode(contractAddress);
		expect(code).to.not.equal("0x");
	});

	step("should handle EIP-7702 transaction type 4 structure", async function () {
		this.timeout(15000);

		// NOTE: This test validates the complete EIP-7702 functionality including:
		// - Authorization creation with proper EIP-7702 signature format
		// - Transaction type 4 creation and sending
		// - Transaction execution and receipt validation

		// Validate prerequisites
		if (!contractAddress) {
			throw new Error("Contract address is required but not set from previous step");
		}

		const authorizer = ethers.Wallet.createRandom();
		const authorization = await authorizer.authorize({
			address: contractAddress,
			nonce: 0,
			chainId: CHAIN_ID,
		});

		// Get current nonce
		const currentNonce = await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT);

		// Attempt to create an EIP-7702 transaction
		const tx = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001", // Some destination
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4, // EIP-7702 transaction type
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [authorization],
			nonce: currentNonce,
		};

		// This test verifies that EIP-7702 transaction structure is recognized and working
		const signedTx = await signer.sendTransaction(tx);
		expect(signedTx.hash).to.be.a("string");

		await createAndFinalizeBlock(context.web3);

		const receipt = await context.ethersjs.getTransactionReceipt(signedTx.hash);
		expect(receipt).to.not.be.null;

		// Verify transaction was executed successfully
		expect(receipt.status).to.equal(1);
	});

	step("should reject empty authorization list", async function () {
		this.timeout(15000);

		// Test with empty authorization list - should be rejected by Frontier
		const tx = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [], // Empty authorization list
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		// Frontier should reject empty authorization lists during validation
		let errorCaught = false;
		try {
			await signer.sendTransaction(tx);
		} catch (error) {
			errorCaught = true;
			// The error could be in different formats, check for the key validation failure
			const errorStr = error.message || error.toString();
			expect(errorStr).to.satisfy(
				(msg: string) =>
					msg.includes("authorization list cannot be empty") ||
					msg.includes("UNKNOWN_ERROR") ||
					msg.includes("authorization")
			);
		}

		// Ensure the error was actually caught
		expect(errorCaught).to.be.true;
	});

	step("should handle authorization with different chain IDs", async function () {
		this.timeout(15000);

		// Test authorization with wrong chain ID - should be skipped by Frontier
		const authorizer = ethers.Wallet.createRandom();
		const wrongChainAuth = await authorizer.authorize({
			address: contractAddress,
			nonce: 0,
			chainId: 999,
		});

		const tx1 = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [wrongChainAuth],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedTx1 = await signer.sendTransaction(tx1);
		await createAndFinalizeBlock(context.web3);

		// Transaction should succeed but authorization should be skipped
		const receipt1 = await context.ethersjs.getTransactionReceipt(signedTx1.hash);
		expect(receipt1.status).to.equal(1);

		// Test authorization with chain ID = 0 (universally valid)
		const universalAuth = await authorizer.authorize({
			address: contractAddress,
			nonce: 1,
			chainId: 0,
		});

		const tx2 = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [universalAuth],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedTx2 = await signer.sendTransaction(tx2);
		await createAndFinalizeBlock(context.web3);

		// Transaction with universal chain ID should succeed
		const receipt2 = await context.ethersjs.getTransactionReceipt(signedTx2.hash);
		expect(receipt2.status).to.equal(1);
	});

	step("should handle multiple authorizations", async function () {
		this.timeout(15000);

		// Create multiple authorizations for the same authority
		const authorizer = ethers.Wallet.createRandom();
		const auth1 = await authorizer.authorize({
			address: contractAddress,
			nonce: 0,
			chainId: CHAIN_ID,
		});

		const auth2 = await authorizer.authorize({
			address: "0x2000000000000000000000000000000000000002",
			nonce: 1,
			chainId: CHAIN_ID,
		});

		const tx = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x200000", // Higher gas for multiple authorizations
			chainId: CHAIN_ID,
			authorizationList: [auth1, auth2],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedTx = await signer.sendTransaction(tx);
		await createAndFinalizeBlock(context.web3);

		const receipt = await context.ethersjs.getTransactionReceipt(signedTx.hash);
		expect(receipt.status).to.equal(1);

		// TODO In Frontier's EIP-7702 implementation, the last valid authorization should take effect
	});

	step("should verify gas cost calculation includes authorization costs", async function () {
		this.timeout(15000);

		// Validate prerequisites
		if (!contractAddress) {
			throw new Error("Contract address is required but not set from previous step");
		}

		const authorizer = ethers.Wallet.createRandom();
		const authorization = await authorizer.authorize({
			address: contractAddress,
			nonce: 0,
			chainId: CHAIN_ID,
		});

		// Instead of using estimateGas (which might fail), execute actual transactions
		// and compare their gas usage

		// Execute regular transaction
		const regularTx = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x100", // Some value
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 2, // EIP-1559 transaction
			gasLimit: "0x5208", // 21000 gas
			chainId: CHAIN_ID,
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const regularSignedTx = await signer.sendTransaction(regularTx);
		await createAndFinalizeBlock(context.web3);
		const regularReceipt = await context.ethersjs.getTransactionReceipt(regularSignedTx.hash);

		// Execute EIP-7702 transaction
		const eip7702Tx = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x100", // Same value
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			authorizationList: [authorization],
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const eip7702SignedTx = await signer.sendTransaction(eip7702Tx);
		await createAndFinalizeBlock(context.web3);
		const eip7702Receipt = await context.ethersjs.getTransactionReceipt(eip7702SignedTx.hash);

		// EIP-7702 transaction should cost more gas due to authorization processing
		expect(Number(eip7702Receipt.gasUsed)).to.be.greaterThan(Number(regularReceipt.gasUsed));
	});

	step("should apply delegation behavior", async function () {
		this.timeout(15000);

		const authorizer = ethers.Wallet.createRandom();
		console.log("Authorizer address:", authorizer.address);
		console.log("Contract address to delegate to:", contractAddress);
		
		const authorization = await authorizer.authorize({
			address: contractAddress,
			nonce: 0,
			chainId: CHAIN_ID,
		});
		console.log("Authorization object:", JSON.stringify(authorization, (key, value) => 
			typeof value === 'bigint' ? value.toString() : value, 2));

		// Set up delegation with a simple call
		const delegationTx = {
			from: GENESIS_ACCOUNT,
			to: authorizer.address,
			data: "0x", // Empty data for simple delegation test
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [authorization],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};
		console.log("Delegation transaction:", JSON.stringify(delegationTx, (key, value) => 
			typeof value === 'bigint' ? value.toString() : value, 2));

		const signedTx = await signer.sendTransaction(delegationTx);
		console.log("Transaction hash:", signedTx.hash);
		await createAndFinalizeBlock(context.web3);

		const receipt = await context.ethersjs.getTransactionReceipt(signedTx.hash);
		console.log("Receipt status:", receipt.status);
		console.log("Receipt logs:", receipt.logs);
		expect(receipt.status).to.equal(1);

		// Check if delegation indicator was set in Frontier
		const accountCode = await context.web3.eth.getCode(authorizer.address);
		console.log("Account code for", authorizer.address, ":", accountCode);
		console.log("Account code length:", accountCode.length);
		const delegationInfo = isDelegationIndicator(accountCode);
		console.log("Delegation info:", delegationInfo);
		expect(delegationInfo.isDelegation).to.be.true;

		// Delegation was set successfully - test calling the simple contract
		const result = await customRequest(context.web3, "eth_call", [
			{
				to: authorizer.address,
				data: "0x", // Empty call data
			},
			"latest",
		]);

		// Simple contract should execute successfully
		// TODO check if the result is as expected
		expect(result.result).to.not.be.null;
	});

	step("should handle delegation edge cases", async function () {
		this.timeout(15000);

		// Test self-delegation (should be prevented by Frontier)
		const authorizer = ethers.Wallet.createRandom();
		const selfDelegationAuth = await authorizer.authorize({
			address: authorizer.address,
			nonce: 0,
			chainId: CHAIN_ID,
		});

		const tx1 = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [selfDelegationAuth],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedTx1 = await signer.sendTransaction(tx1);
		await createAndFinalizeBlock(context.web3);

		// Self-delegation should be handled gracefully by Frontier
		const receipt1 = await context.ethersjs.getTransactionReceipt(signedTx1.hash);
		expect(receipt1.status).to.equal(1);

		// Test delegation to zero address
		const zeroAddressAuth = await authorizer.authorize({
			address: "0x0000000000000000000000000000000000000000",
			nonce: 0,
			chainId: CHAIN_ID,
		});

		const tx2 = {
			from: GENESIS_ACCOUNT,
			to: "0x1000000000000000000000000000000000000001",
			value: "0x00",
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4,
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [zeroAddressAuth],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedTx2 = await signer.sendTransaction(tx2);
		await createAndFinalizeBlock(context.web3);

		// Zero address delegation should be handled by Frontier
		const receipt2 = await context.ethersjs.getTransactionReceipt(signedTx2.hash);
		expect(receipt2.status).to.equal(1);
	});

	step("happy path: complete EIP-7702 delegation workflow", async function () {
		this.timeout(20000);

		// This test demonstrates the complete happy path for EIP-7702 delegation:
		// 1. Create a new EOA that will delegate to a smart contract
		// 2. Fund the EOA
		// 3. Create and submit a delegation authorization
		// 4. Verify the delegation was successful
		// 5. Call a function through the delegated EOA

		// Step 1: Create a new EOA
		const delegatorAccount = ethers.Wallet.createRandom();
		const delegatorAddress = delegatorAccount.address;

		// Step 2: Fund the EOA
		const fundingTx = await signer.sendTransaction({
			to: delegatorAddress,
			value: ethers.parseEther("1.0"), // Send 1 ETH
			gasLimit: "0x5208",
			gasPrice: "0x3B9ACA00",
		});
		await createAndFinalizeBlock(context.web3);

		const fundingReceipt = await context.ethersjs.getTransactionReceipt(fundingTx.hash);
		expect(fundingReceipt.status).to.equal(1);

		// Verify balance
		const balance = await context.web3.eth.getBalance(delegatorAddress);
		expect(BigInt(balance)).to.equal(BigInt(ethers.parseEther("1.0")));

		// Step 3: Create authorization to delegate to the test contract
		const delegatorCurrentNonce = await context.ethersjs.getTransactionCount(delegatorAddress);
		const authorization = await delegatorAccount.authorize({
			address: contractAddress,
			nonce: delegatorCurrentNonce,
			chainId: CHAIN_ID,
		});

		// Submit the delegation transaction (first transaction - simple transfer)
		const randomRecipient = ethers.Wallet.createRandom().address;
		const delegationTx = {
			from: GENESIS_ACCOUNT,
			to: randomRecipient, // Send to a random account
			value: "0x100", // Small transfer amount
			maxFeePerGas: "0x3B9ACA00",
			maxPriorityFeePerGas: "0x01",
			type: 4, // EIP-7702 transaction type
			gasLimit: "0x100000",
			chainId: CHAIN_ID,
			authorizationList: [authorization],
			nonce: await context.ethersjs.getTransactionCount(GENESIS_ACCOUNT),
		};

		const signedDelegationTx = await signer.sendTransaction(delegationTx);
		await createAndFinalizeBlock(context.web3);

		const delegationReceipt = await context.ethersjs.getTransactionReceipt(signedDelegationTx.hash);
		expect(delegationReceipt.status).to.equal(1);
		expect(delegationReceipt.logs).to.be.an("array");

		// Step 4: Verify delegation by checking the account code
		const accountCode = await context.web3.eth.getCode(delegatorAddress);
		console.log("Account code:", accountCode);
		console.log("Account code length:", accountCode.length);
		const delegationInfo = isDelegationIndicator(accountCode);
		console.log("Delegation info:", delegationInfo);

		// Expect delegation to be set
		expect(delegationInfo.isDelegation).to.be.true;
		expect(delegationInfo.address.toLowerCase()).to.equal(contractAddress.toLowerCase());

		// Step 5: Call the delegated contract (second transaction - invoke code at address with delegation indicator)
		const callTx = await signer.sendTransaction({
			to: delegatorAddress,
			data: "0x", // Empty data for simple contract call
			gasLimit: "0x100000",
			gasPrice: "0x3B9ACA00",
		});

		await createAndFinalizeBlock(context.web3);

		const callReceipt = await context.ethersjs.getTransactionReceipt(callTx.hash);
		expect(callReceipt.status).to.equal(1);

		// Verify the delegator account still has its balance
		const finalBalance = await context.web3.eth.getBalance(delegatorAddress);
		expect(Number(finalBalance)).to.be.greaterThan(0);
	});
});
