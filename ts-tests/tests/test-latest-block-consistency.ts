import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

// Consistency tests for "latest" RPC responses. This suite validates that
// eth_getBlockByNumber("latest") remains non-null and consistent with related RPCs.
// Note: this harness uses --tmp nodes, so it cannot fully simulate an upgrade with
// an existing DB; these checks focus on externally visible consistency guarantees.
describeWithFrontier("Frontier RPC (Latest Block Consistency)", (context) => {
	step("eth_getBlockByNumber('latest') should return a non-null block after node start", async function () {
		const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(block).to.not.be.null;
		expect(parseInt(block.number, 16)).to.be.gte(0);
	});

	step("eth_blockNumber should match eth_getBlockByNumber('latest') after node start", async function () {
		// Verify eth_blockNumber is consistent with eth_getBlockByNumber("latest")
		const blockNumber = await context.web3.eth.getBlockNumber();
		const latestBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		expect(Number(blockNumber)).to.equal(parseInt(latestBlock.number, 16));
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
