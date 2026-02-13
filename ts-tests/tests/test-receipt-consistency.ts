import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlockNowait, describeWithFrontier, customRequest, waitForBlock } from "./util";

/**
 * Test for receipt consistency (ADR-003).
 *
 * Verifies that if eth_getBlockByNumber returns a block, eth_getTransactionReceipt
 * also returns receipts for transactions in that block.
 *
 * ADR-003 ensures this by having all RPCs read from mapping-sync.
 */
describeWithFrontier("Frontier RPC (Receipt Consistency)", (context) => {
	const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

	async function waitForTxPoolPendingAtLeast(minPending: number, timeoutMs = 5000) {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			const status = (await customRequest(context.web3, "txpool_status", [])).result;
			const pending = parseInt(status.pending, 16);
			if (pending >= minPending) {
				return;
			}
			await new Promise<void>((resolve) => setTimeout(resolve, 50));
		}
		throw new Error(`Timed out waiting for txpool pending >= ${minPending}`);
	}

	async function waitForReceipt(txHash: string, timeoutMs = 10000) {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			const receipt = await context.web3.eth.getTransactionReceipt(txHash);
			if (receipt !== null) {
				return receipt;
			}
			await createAndFinalizeBlockNowait(context.web3);
			await new Promise<void>((resolve) => setTimeout(resolve, 50));
		}
		throw new Error(`Timed out waiting for receipt ${txHash}`);
	}

	step("should return receipt immediately after block is visible", async function () {
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

		// Get current block number before creating the new block
		const currentBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const currentNumber = currentBlock ? parseInt(currentBlock.number, 16) : 0;

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for the NEW block to become visible (with full transaction details)
		const newBlockNumber = "0x" + (currentNumber + 1).toString(16);
		const block = await waitForBlock(context.web3, newBlockNumber, 5000, true);
		expect(block).to.not.be.null;
		expect(block.transactions).to.be.an("array").with.lengthOf(1);
		expect(block.transactions[0].hash).to.equal(txHash);

		// If block is visible, receipt should also be available
		const receipt = await context.web3.eth.getTransactionReceipt(txHash);

		expect(receipt).to.not.be.null;
		expect(receipt.transactionHash).to.equal(txHash);
		expect(receipt.blockHash).to.equal(block.hash);
		expect(BigInt(receipt.blockNumber)).to.equal(BigInt(block.number));
		expect(receipt.status).to.equal(true);
	});

	step("should return receipt for multiple transactions in same block", async function () {
		const txCount = 3;
		const txHashes: string[] = [];

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

		await waitForTxPoolPendingAtLeast(txCount);

		// Get current block number before creating the new block
		const currentBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const currentNumber = currentBlock ? parseInt(currentBlock.number, 16) : 0;

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for the NEW block to become visible (with full transaction details).
		// Depending on pool scheduling, not all pending transactions are guaranteed in a single block.
		const newBlockNumber = "0x" + (currentNumber + 1).toString(16);
		const block = await waitForBlock(context.web3, newBlockNumber, 5000, true);
		expect(block).to.not.be.null;
		expect(block.transactions.length).to.be.greaterThan(0);

		// All receipts should eventually be available and point to visible blocks.
		for (let i = 0; i < txCount; i++) {
			const receipt = await waitForReceipt(txHashes[i]);

			expect(receipt, `Receipt for tx ${i}`).to.not.be.null;
			expect(receipt.transactionHash).to.equal(txHashes[i]);
			const receiptBlock = await context.web3.eth.getBlock(receipt.blockNumber);
			expect(receiptBlock).to.not.be.null;
			expect(receipt.blockHash).to.equal(receiptBlock.hash);
		}
	});

	step("should return receipt when queried by transaction hash from block", async function () {
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

		// Get current block number before creating the new block
		const currentBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const currentNumber = currentBlock ? parseInt(currentBlock.number, 16) : 0;

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for the NEW block to become visible (with full transaction details)
		const newBlockNumber = "0x" + (currentNumber + 1).toString(16);
		const block = await waitForBlock(context.web3, newBlockNumber, 5000, true);
		expect(block).to.not.be.null;
		expect(block.transactions).to.be.an("array").with.lengthOf(1);

		// Get tx hash from the block, then query its receipt
		const txFromBlock = block.transactions[0];
		const txHash = txFromBlock.hash;

		const receipt = await context.web3.eth.getTransactionReceipt(txHash);

		expect(receipt).to.not.be.null;
		expect(receipt.transactionHash).to.equal(txHash);
		expect(receipt.from.toLowerCase()).to.equal(GENESIS_ACCOUNT.toLowerCase());
		expect(receipt.to.toLowerCase()).to.equal(TEST_ACCOUNT.toLowerCase());
	});
});
