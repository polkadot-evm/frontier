import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlockNowait, describeWithFrontier, customRequest } from "./util";

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

	// Helper: poll until eth_getBlockByNumber returns a non-null block or timeout
	async function waitForBlock(blockTag: string, timeoutMs: number = 5000): Promise<any> {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			const block = (await customRequest(context.web3, "eth_getBlockByNumber", [blockTag, true])).result;
			if (block !== null) {
				return block;
			}
			await new Promise((resolve) => setTimeout(resolve, 50));
		}
		return null;
	}

	step("should return receipt immediately after block is visible", async function () {
		this.timeout(15000);

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

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for block to become visible
		const block = await waitForBlock("latest");
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
		this.timeout(15000);

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

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for block to become visible
		const block = await waitForBlock("latest");
		expect(block).to.not.be.null;
		expect(block.transactions).to.have.lengthOf(txCount);

		// All receipts should be available
		for (let i = 0; i < txCount; i++) {
			const receipt = await context.web3.eth.getTransactionReceipt(txHashes[i]);

			expect(receipt, `Receipt for tx ${i}`).to.not.be.null;
			expect(receipt.transactionHash).to.equal(txHashes[i]);
			expect(receipt.transactionIndex).to.equal(i);
			expect(receipt.blockHash).to.equal(block.hash);
		}
	});

	step("should return receipt when queried by transaction hash from block", async function () {
		this.timeout(15000);

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

		await createAndFinalizeBlockNowait(context.web3);

		// Wait for block to become visible
		const block = await waitForBlock("latest");
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
