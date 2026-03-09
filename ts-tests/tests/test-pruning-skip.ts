import { expect } from "chai";
import { step } from "mocha-steps";

import {
	createAndFinalizeBlock,
	createAndFinalizeBlockNowait,
	describeWithFrontier,
	customRequest,
	waitForBlock,
} from "./util";

// Integration test for KV mapping-sync pruning-skip behavior. When state pruning
// is enabled, the sync worker may have tips behind the live window (finalized - N).
// The worker skips those tips and continues from the window start; in-window tips
// must be retained so sync catches up. This test runs the node with --state-pruning
// and asserts that "latest" stays valid and catches up after a burst of blocks.
const STATE_PRUNING_BLOCKS = 64;
const BLOCKS_PAST_WINDOW = 80;
const BURST_SIZE = 24;

describeWithFrontier(
	"Frontier KV mapping-sync (pruning skip / tip retention)",
	(context) => {
		step("should index genesis with state pruning enabled", async function () {
			const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["0x0", false])).result;
			expect(block).to.not.be.null;
		});

		step("should produce blocks past the pruning window and finalize", async function () {
			this.timeout(120_000);
			const start = Number(await context.web3.eth.getBlockNumber());
			for (let i = 0; i < BLOCKS_PAST_WINDOW - start; i++) {
				await createAndFinalizeBlock(context.web3);
			}
			const end = Number(await context.web3.eth.getBlockNumber());
			expect(end).to.be.gte(BLOCKS_PAST_WINDOW);
		});

		step("should keep latest non-null and catch up after a burst of blocks", async function () {
			this.timeout(60_000);
			const indexedBefore = Number(await context.web3.eth.getBlockNumber());
			for (let i = 0; i < BURST_SIZE; i++) {
				await createAndFinalizeBlockNowait(context.web3);
			}
			const expectedIndexed = indexedBefore + BURST_SIZE;
			const expectedTag = "0x" + expectedIndexed.toString(16);

			// During lag, latest should still be non-null (worker may skip pruned tips).
			const latestDuring = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
			expect(latestDuring).to.not.be.null;
			expect(parseInt(latestDuring.number, 16)).to.be.at.most(expectedIndexed);

			// Catch-up: wait for the last block to be indexed.
			await waitForBlock(context.web3, expectedTag, 30_000);
			const latestAfter = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
			expect(latestAfter).to.not.be.null;
			expect(parseInt(latestAfter.number, 16)).to.equal(expectedIndexed);
		});
	},
	undefined,
	["--state-pruning", String(STATE_PRUNING_BLOCKS)]
);
