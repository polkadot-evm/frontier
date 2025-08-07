import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithTokfinAllPools } from "./util";

describeWithTokfinAllPools("Tokfin RPC (Pending Transactions)", (context) => {
	const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

	// Helper function to create and send a transaction
	async function sendTransaction(nonce?: number, options = {}) {
		const defaultTxParams: {
			from: string;
			to: string;
			data: string;
			value: string;
			gasPrice: string;
			gas: string;
			nonce?: number;
		} = {
			from: GENESIS_ACCOUNT,
			to: TEST_ACCOUNT,
			data: "0x00",
			value: "0x200", // Must be higher than ExistentialDeposit
			gasPrice: "0x3B9ACA00",
			gas: "0x100000",
		};

		// Use next available nonce if not provided
		const txParams = { ...defaultTxParams, ...options };
		if (nonce !== undefined) {
			txParams.nonce = nonce;
		}

		const tx = await context.web3.eth.accounts.signTransaction(txParams, GENESIS_ACCOUNT_PRIVATE_KEY);

		const result = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		return {
			hash: result.result,
			...txParams,
		};
	}

	// Helper to get pending transactions
	async function getPendingTransactions() {
		const response = await customRequest(context.web3, "eth_pendingTransactions", []);
		return response.result || [];
	}

	step("should return empty array when no transactions are pending", async function () {
		const pendingTransactions = await getPendingTransactions();
		expect(pendingTransactions).to.be.an("array").that.is.empty;
	});

	step("should return pending transactions when transactions are in mempool", async function () {
		// First, create a block to clear previous pending transactions
		await createAndFinalizeBlock(context.web3);

		const readyTransactionCount = 3;
		const futureTransactionCount = 2;
		const transactions = [];

		// Get initial nonce
		const initialNonce = await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT);

		// Submit regular transactions with sequential nonces
		for (let i = 0; i < readyTransactionCount; i++) {
			const currentNonce = initialNonce + i;
			const tx = await sendTransaction(currentNonce);
			transactions.push(tx);
		}

		// Submit future transactions with gaps in nonces
		for (let i = 0; i < futureTransactionCount; i++) {
			// Create a gap by skipping some nonces
			const gapSize = i * 2 + 1;
			const futureNonce = initialNonce + readyTransactionCount + gapSize;
			const tx = await sendTransaction(futureNonce);
			transactions.push(tx);
		}

		// Check pending transactions through RPC
		const pendingTransactions = await getPendingTransactions();

		// Verify the response
		expect(pendingTransactions).to.be.an("array");
		expect(pendingTransactions.length).to.equal(transactions.length);

		// Verify transaction hashes match what we submitted
		const pendingHashes = pendingTransactions.map((tx) => tx.hash);
		const submittedHashes = transactions.map((tx) => tx.hash);
		expect(pendingHashes).to.have.members(submittedHashes);
	});

	step("should remove transactions from pending transactions when block is created", async function () {
		// First, create a block to clear previous pending transactions
		await createAndFinalizeBlock(context.web3);

		// Get current nonce
		const nonce = await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT);

		// Submit a transaction
		await sendTransaction(nonce, {
			gasPrice: context.web3.utils.toWei("1", "gwei"),
		});

		// Check that it's in the pending transactions
		const pendingBefore = await getPendingTransactions();
		expect(pendingBefore.length).to.be.at.least(1);
		const countBefore = pendingBefore.length;

		// Create a block to mine the pending transactions
		await createAndFinalizeBlock(context.web3);

		// Check pending transactions again
		const pendingAfter = await getPendingTransactions();

		// Verify there are fewer pending transactions after mining
		expect(pendingAfter.length).to.be.lessThan(countBefore);
	});
});
