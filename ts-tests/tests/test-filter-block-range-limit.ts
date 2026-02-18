import { expect } from "chai";
import { step } from "mocha-steps";

import {
	createAndFinalizeBlock,
	describeWithFrontier,
	customRequest,
	waitForBlock,
} from "./util";

const MAX_BLOCK_RANGE = 10;
const BLOCK_RANGE_ERROR_MSG = `block range is too wide (maximum ${MAX_BLOCK_RANGE})`;

describeWithFrontier(
	"Frontier RPC (Filter/GetLogs block range limit)",
	(context) => {
		step("eth_getLogs with wide block range should be rejected", async function () {
			// Produce enough blocks so the requested range is not capped by chain height.
			// Requested range must exceed MAX_BLOCK_RANGE so the server rejects it.
			const blocksToProduce = MAX_BLOCK_RANGE + 5;
			for (let i = 0; i < MAX_BLOCK_RANGE + 5; i++) {
				await createAndFinalizeBlock(context.web3);
			}
			await waitForBlock(
				context.web3,
				"0x" + blocksToProduce.toString(16),
				10000
			);

			const r = await customRequest(context.web3, "eth_getLogs", [
				{
					fromBlock: "0x0",
					toBlock: "0x" + blocksToProduce.toString(16), // range = blocksToProduce > MAX_BLOCK_RANGE
				},
			]);
			expect(r.error).to.not.be.undefined;
			expect(r.error.message).to.include(BLOCK_RANGE_ERROR_MSG);
		});

		step("eth_newFilter with wide numeric block range should be rejected", async function () {
			const r = await customRequest(context.web3, "eth_newFilter", [
				{
					fromBlock: "0x0",
					toBlock: "0x20", // 32 blocks > MAX_BLOCK_RANGE
					address: "0x0000000000000000000000000000000000000000",
				},
			]);
			expect(r.error).to.not.be.undefined;
			expect(r.error.message).to.include(BLOCK_RANGE_ERROR_MSG);
		});

		step("eth_newFilter with valid block range should succeed", async function () {
			const r = await customRequest(context.web3, "eth_newFilter", [
				{
					fromBlock: "0x0",
					toBlock: "0x5", // 5 blocks <= MAX_BLOCK_RANGE
					address: "0x0000000000000000000000000000000000000000",
				},
			]);
			expect(r.error).to.be.undefined;
			expect(r.result).to.not.be.undefined;
		});

		step("eth_getFilterLogs with effective range exceeding limit should be rejected", async function () {
			// Create filter when chain has few blocks (e.g. 2–3 after genesis).
			const b = await context.web3.eth.getBlockNumber();
			const fromBlock = `0x${(b - MAX_BLOCK_RANGE).toString(16)}`;
			const createFilter = await customRequest(context.web3, "eth_newFilter", [
				{
					fromBlock,
					toBlock: "latest",
					address: "0x0000000000000000000000000000000000000000",
				},
			]);
			expect(createFilter.error).to.be.undefined;
			const filterId = createFilter.result;

			// Produce enough blocks so effective range (0 to latest) > MAX_BLOCK_RANGE.
			for (let i = 0; i < MAX_BLOCK_RANGE; i++) {
				await createAndFinalizeBlock(context.web3);
			}
			await waitForBlock(context.web3, "latest", 5000);

			const poll = await customRequest(context.web3, "eth_getFilterLogs", [
				filterId,
			]);
			expect(poll.error).to.not.be.undefined;
			expect(poll.error.message).to.include(BLOCK_RANGE_ERROR_MSG);
		});

		step("eth_getFilterChanges with effective range exceeding limit should be rejected", async function () {
			const b = await context.web3.eth.getBlockNumber();
			const fromBlock = `0x${(b - MAX_BLOCK_RANGE).toString(16)}`;
			const createFilter = await customRequest(context.web3, "eth_newFilter", [
				{
					fromBlock,
					toBlock: "latest",
					address: "0x0000000000000000000000000000000000000000",
				},
			]);
			expect(createFilter.error).to.be.undefined;
			const filterId = createFilter.result;

			// Produce one more than MAX_BLOCK_RANGE so the poll range (last_poll..latest)
			// exceeds the limit. getFilterChanges uses from_number = max(last_poll, filter_from);
			// at creation last_poll = best (e.g. 25), so after N blocks range = N (25 to 25+N).
			// We need N > MAX_BLOCK_RANGE.
			for (let i = 0; i < MAX_BLOCK_RANGE + 1; i++) {
				await createAndFinalizeBlock(context.web3);
			}
			await waitForBlock(context.web3, "latest", 5000);

			const poll = await customRequest(context.web3, "eth_getFilterChanges", [
				filterId,
			]);
			expect(poll.error).to.not.be.undefined;
			expect(poll.error.message).to.include(BLOCK_RANGE_ERROR_MSG);
		});
	},
	undefined,
	[`--max-block-range=${MAX_BLOCK_RANGE}`]
);
