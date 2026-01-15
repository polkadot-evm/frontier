import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlockNowait, describeWithFrontier, customRequest } from "./util";

/**
 * Test for receipt consistency issue (ADR-001).
 *
 * This test verifies that eth_getTransactionReceipt returns a valid receipt
 * immediately after getting a transaction hash from eth_getBlockByNumber.
 *
 * The race condition occurs because:
 * - eth_getBlockByNumber reads from runtime storage (available immediately after import)
 * - eth_getTransactionReceipt reads from mapping-sync database (delayed)
 *
 * The fix implements a runtime storage fallback when the mapping-sync database
 * doesn't have the transaction indexed yet.
 */
describeWithFrontier("Frontier RPC (Receipt Consistency)", (context) => {
	const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

	step("should return receipt immediately after block is visible", async function () {
		this.timeout(15000);

		// Send a transaction
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: TEST_ACCOUNT,
				value: "0x200",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
				nonce: 0,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;
		expect(txHash).to.be.a("string").lengthOf(66);

		// Create block WITHOUT waiting for mapping-sync (no 500ms sleep)
		await createAndFinalizeBlockNowait(context.web3);

		// Immediately get the block - this uses runtime storage
		const block = await context.web3.eth.getBlock("latest", false);
		expect(block).to.not.be.null;
		expect(block.transactions).to.be.an("array").with.lengthOf(1);
		expect(block.transactions[0]).to.equal(txHash);

		// Immediately get the receipt - this should work even before mapping-sync completes
		// This is the core assertion: if we can see the tx in the block, we should be able to get its receipt
		const receipt = await context.web3.eth.getTransactionReceipt(txHash);

		expect(receipt).to.not.be.null;
		expect(receipt.transactionHash).to.equal(txHash);
		expect(receipt.blockHash).to.equal(block.hash);
		expect(receipt.blockNumber).to.equal(block.number);
		expect(receipt.status).to.equal(true);
	});

	step("should return receipt for multiple transactions in same block", async function () {
		this.timeout(15000);

		const txCount = 3;
		const txHashes: string[] = [];

		// Send multiple transactions
		for (let i = 0; i < txCount; i++) {
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: TEST_ACCOUNT,
					value: "0x200",
					gasPrice: "0x3B9ACA00",
					gas: "0x100000",
					nonce: i + 1, // nonce 0 was used in previous test
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);

			const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;
			txHashes.push(txHash);
		}

		// Create block without waiting
		await createAndFinalizeBlockNowait(context.web3);

		// Get block and verify transactions are there
		const block = await context.web3.eth.getBlock("latest", false);
		expect(block.transactions).to.have.lengthOf(txCount);

		// Verify all receipts are immediately available
		for (let i = 0; i < txCount; i++) {
			const receipt = await context.web3.eth.getTransactionReceipt(txHashes[i]);

			expect(receipt, `Receipt for tx ${i} should not be null`).to.not.be.null;
			expect(receipt.transactionHash).to.equal(txHashes[i]);
			expect(receipt.transactionIndex).to.equal(i);
			expect(receipt.blockHash).to.equal(block.hash);
		}
	});

	step("should return receipt when queried by transaction hash from block", async function () {
		this.timeout(15000);

		// Send a transaction
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: TEST_ACCOUNT,
				value: "0x200",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
				nonce: 4, // continuing from previous tests
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);

		// Create block without waiting
		await createAndFinalizeBlockNowait(context.web3);

		// Get block with FULL transaction objects
		const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", true])).result;
		expect(block.transactions).to.be.an("array").with.lengthOf(1);

		// Extract tx hash from the block response
		const txFromBlock = block.transactions[0];
		const txHash = txFromBlock.hash;

		// Get receipt using the hash we got from the block
		// This is the exact user scenario: get tx from block, then query its receipt
		const receipt = await context.web3.eth.getTransactionReceipt(txHash);

		expect(receipt).to.not.be.null;
		expect(receipt.transactionHash).to.equal(txHash);
		expect(receipt.from.toLowerCase()).to.equal(GENESIS_ACCOUNT.toLowerCase());
		expect(receipt.to.toLowerCase()).to.equal(TEST_ACCOUNT.toLowerCase());
	});
});
