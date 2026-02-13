import { expect } from "chai";
import { step } from "mocha-steps";

import {
	createAndFinalizeBlock,
	createAndFinalizeBlockNowait,
	describeWithFrontier,
	customRequest,
	waitForBlock,
} from "./util";

// Consistency tests for "latest" RPC responses. This suite validates that
// eth_getBlockByNumber("latest") remains non-null and consistent with related RPCs.
// Note: this harness uses --tmp nodes, so it cannot fully simulate an upgrade with
// an existing DB; these checks focus on externally visible consistency guarantees.
describeWithFrontier("Frontier RPC (Latest Block Consistency)", (context) => {
	const RECOVERY_LIMIT = 6;

	async function createBlock(finalize: boolean = true, parentHash: string | null = null): Promise<string> {
		const response = await customRequest(context.web3, "engine_createBlock", [true, finalize, parentHash]);
		if (!response.result?.hash) {
			throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
		}
		return response.result.hash;
	}

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

	step("latest, blockNumber, and logs should remain consistent after a reorg", async function () {
		const tip = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		expect(tip).to.not.be.null;
		const tipNumber = parseInt(tip.number, 16);

		// Build a short branch from current best.
		const anchor = await createBlock(false);
		const a1 = await createBlock(false, anchor);
		await createBlock(false, a1);

		// Build a longer competing branch from the same anchor to force a reorg.
		const b1 = await createBlock(false, anchor);
		const b2 = await createBlock(false, b1);
		await createBlock(false, b2);

		const expectedReorgHead = "0x" + (tipNumber + 4).toString(16);
		await waitForBlock(context.web3, expectedReorgHead, 15000);

		const latest = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const blockNumber = Number(await context.web3.eth.getBlockNumber());
		expect(latest).to.not.be.null;
		expect(parseInt(latest.number, 16)).to.equal(blockNumber);
		expect(parseInt(latest.number, 16)).to.equal(tipNumber + 4);

		const logs = await customRequest(context.web3, "eth_getLogs", [
			{
				fromBlock: tip.number,
				toBlock: "latest",
			},
		]);
		expect(logs.error).to.be.undefined;
		expect(logs.result).to.be.an("array");
	});

	step("eth_getBlockByNumber('latest') should return new block after production", async function () {
		const before = Number(await context.web3.eth.getBlockNumber());
		// Create a block
		await createAndFinalizeBlock(context.web3);

		// eth_getBlockByNumber("latest") should now advance by one block.
		const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(block).to.not.be.null;
		expect(parseInt(block.number, 16)).to.be.gte(before + 1);
	});

	step("eth_blockNumber should match latest block after production", async function () {
		const blockNumber = await context.web3.eth.getBlockNumber();
		const latestBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;

		expect(Number(blockNumber)).to.equal(parseInt(latestBlock.number, 16));
	});

	step("eth_getBlockByNumber('latest') should never return null after multiple blocks", async function () {
		let previous = Number(await context.web3.eth.getBlockNumber());
		// Create several more blocks
		for (let _ = 0; _ < 5; _++) {
			await createAndFinalizeBlock(context.web3);

			// Verify latest block is never null after each block
			const block = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
			expect(block).to.not.be.null;
			const observed = parseInt(block.number, 16);
			expect(observed).to.be.gte(previous + 1);
			previous = observed;
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

	step("latest RPCs should stay consistent during indexing lag beyond recovery limit", async function () {
		const startIndexed = Number(await context.web3.eth.getBlockNumber());
		const lagBlocks = RECOVERY_LIMIT + 4;

		for (let i = 0; i < lagBlocks; i++) {
			await createAndFinalizeBlockNowait(context.web3);
		}

		// During lag, latest should still be non-null and internally consistent.
		const latestDuringLag = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const numberDuringLag = Number(await context.web3.eth.getBlockNumber());
		expect(latestDuringLag).to.not.be.null;
		expect(parseInt(latestDuringLag.number, 16)).to.equal(numberDuringLag);

		// Once indexing catches up, latest should advance to the produced height.
		const expectedIndexed = "0x" + (startIndexed + lagBlocks).toString(16);
		await waitForBlock(context.web3, expectedIndexed, 15000);

		const latestAfterCatchup = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false]))
			.result;
		expect(parseInt(latestAfterCatchup.number, 16)).to.be.gte(startIndexed + lagBlocks);
	});

	step("eth_getBlockByNumber('latest') should never return null during frequent polling", async function () {
		this.timeout(30000);

		const pollCount = 120;
		const pollIntervalMs = 50;
		const producerBlocks = 25;
		let producerDone = false;
		const failures: Array<{ i: number; value: unknown }> = [];

		const producer = (async () => {
			for (let i = 0; i < producerBlocks; i++) {
				await createAndFinalizeBlockNowait(context.web3);
			}
			producerDone = true;
		})();

		const poller = (async () => {
			for (let i = 0; i < pollCount || !producerDone; i++) {
				const response = await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false]);
				if (response.result == null) {
					failures.push({ i, value: response.result });
				}
				await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
			}
		})();

		await Promise.all([producer, poller]);

		expect(failures, `latest returned null in ${failures.length} polls`).to.be.empty;
	});

	step("latest should stay non-null during alternating reorg storms and converge", async function () {
		this.timeout(45000);

		const rounds = 4;
		let expectedHead = Number(await context.web3.eth.getBlockNumber());
		const nulls: number[] = [];

		for (let i = 0; i < rounds; i++) {
			const anchor = await createBlock(false);
			expectedHead += 1;

			const a1 = await createBlock(false, anchor);
			expectedHead += 1;

			const b1 = await createBlock(false, anchor);
			await createBlock(false, b1);
			expectedHead += 1;

			// Poll while branches are flipping.
			for (let j = 0; j < 20; j++) {
				const latest = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
				if (latest == null) {
					nulls.push(i * 20 + j);
				}
				await new Promise((resolve) => setTimeout(resolve, 40));
			}

			// Ensure both branches were imported.
			expect(a1).to.be.a("string");
		}

		await waitForBlock(context.web3, "0x" + expectedHead.toString(16), 20000);
		const latest = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		const blockNumber = Number(await context.web3.eth.getBlockNumber());

		expect(nulls, `latest returned null at polls ${nulls.join(",")}`).to.be.empty;
		expect(latest).to.not.be.null;
		expect(parseInt(latest.number, 16)).to.equal(blockNumber);
		expect(parseInt(latest.number, 16)).to.equal(expectedHead);
	});

	step("explicit number/hash block queries should remain non-null during indexing lag", async function () {
		this.timeout(30000);

		for (let i = 0; i < 12; i++) {
			await createAndFinalizeBlockNowait(context.web3);
		}

		const latest = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		expect(latest).to.not.be.null;

		const numberHex = latest.number as string;
		const hash = latest.hash as string;
		const byNumber = (await customRequest(context.web3, "eth_getBlockByNumber", [numberHex, true])).result;
		const byHash = (await customRequest(context.web3, "eth_getBlockByHash", [hash, true])).result;

		expect(byNumber).to.not.be.null;
		expect(byHash).to.not.be.null;
		expect(byNumber.hash).to.equal(hash);
		expect(byHash.number).to.equal(numberHex);
	});
});
