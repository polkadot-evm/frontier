import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier, customRequest, waitForBlock } from "./util";

// Test for upgrade/restart behavior:
// Verifies that eth_getBlockByNumber("latest") works correctly when:
// - The node starts with an existing synced DB
// - No new blocks have been imported yet
// - The LATEST_CANONICAL_INDEXED_BLOCK key may not exist (pre-upgrade DB)
//
// In this scenario, the RPC should return a valid block (genesis) rather than null.
// This test simulates the scenario by verifying behavior immediately after node start
// and after block creation.
describeWithFrontier("Frontier RPC (Latest Block Consistency)", (context) => {
	step("eth_getBlockByNumber('latest') should return genesis immediately after node start", async function () {
		// This tests the scenario where LATEST_CANONICAL_INDEXED_BLOCK may not be set
		// (e.g., after upgrade/restart before mapping-sync processes new blocks).
		// The node should return genesis, not null.
		const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(block).to.not.be.null;
		expect(block.number).to.equal("0x0");
	});

	step("eth_blockNumber should return 0 immediately after node start", async function () {
		// Verify eth_blockNumber is consistent with eth_getBlockByNumber("latest")
		const blockNumber = await context.web3.eth.getBlockNumber();
		expect(Number(blockNumber)).to.equal(0);
	});

	step("eth_coinbase should work immediately after node start", async function () {
		// eth_coinbase depends on latest block, verify it doesn't fail
		const result = await customRequest(context.web3, "eth_coinbase", []);
		// Should return a valid address (even if zero address), not an error
		expect(result.error).to.be.undefined;
		expect(result.result)
			.to.be.a("string")
			.that.matches(/^0x[0-9a-fA-F]{40}$/);
	});

	step("eth_getBlockByNumber('latest') should return new block after production", async function () {
		// Create a block
		await createAndFinalizeBlock(context.web3);

		// eth_getBlockByNumber("latest") should now return block 1
		const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(block).to.not.be.null;
		expect(block.number).to.equal("0x1");
	});

	step("eth_blockNumber should match latest block after production", async function () {
		const blockNumber = await context.web3.eth.getBlockNumber();
		const latestBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(Number(blockNumber)).to.equal(parseInt(latestBlock.number, 16));
	});

	step("eth_getBlockByNumber('latest') should never return null after multiple blocks", async function () {
		// Create several more blocks
		for (let i = 0; i < 5; i++) {
			await createAndFinalizeBlock(context.web3);

			// Verify latest block is never null after each block
			const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
			expect(block).to.not.be.null;
			expect(parseInt(block.number, 16)).to.equal(2 + i);
		}
	});

	step("eth_getLogs should work with 'latest' tag", async function () {
		// eth_getLogs depends on latest block calculation, verify it works
		const result = await customRequest(context.web3, "eth_getLogs", [
			{
				fromBlock: "0x0",
				toBlock: "latest",
			},
		]);

		expect(result.error).to.be.undefined;
		expect(result.result).to.be.an("array");
	});
});
