import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier, customRequest, waitForBlock } from "./util";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";

// Integration test: after a reorg, eth_getBlockByNumber must return the winning
// fork's block content and transactions from the losing fork must not reference
// the old (reorged-out) block.
describeWithFrontier("Frontier RPC (Reorg Block & Transaction Consistency)", (context) => {
	async function createBlock(finalize: boolean = true, parentHash: string | null = null): Promise<string> {
		const response = await customRequest(context.web3, "engine_createBlock", [true, finalize, parentHash]);
		if (!response.result?.hash) {
			throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
		}
		return response.result.hash as string;
	}

	async function sendTransfer(nonce: number, value: string = "0x1"): Promise<string> {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: "0x0000000000000000000000000000000000000001",
				value,
				gas: "0x5208",
				gasPrice: "0x3B9ACA00",
				nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const result = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		return result.result as string;
	}

	step("eth_getBlockByNumber should return the winning fork's block after a reorg", async function () {
		this.timeout(60_000);

		// Advance a few blocks to have a stable starting point.
		for (let i = 0; i < 3; i++) {
			await createAndFinalizeBlock(context.web3);
		}

		// Capture the current best block number BEFORE creating any forks.
		const tipBlock = (await customRequest(context.web3, "eth_getBlockByNumber", ["latest", false])).result;
		expect(tipBlock).to.not.be.null;
		const tipNumber = parseInt(tipBlock.number, 16);

		// Create the fork anchor (non-finalized). It returns a substrate hash
		// that we can use as the parent for competing branches.
		const anchor = await createBlock(false);
		const reorgHeight = tipNumber + 2;

		// --- Fork A: 2 blocks ---
		const a1 = await createBlock(false, anchor);
		await createBlock(false, a1);

		// Wait for fork A to be visible at the reorg height, then capture
		// the Ethereum block hash (which should belong to fork A right now).
		const reorgHeightHex = "0x" + reorgHeight.toString(16);
		await waitForBlock(context.web3, reorgHeightHex, 10_000);
		const ethBlockForkA = (await customRequest(context.web3, "eth_getBlockByNumber", [reorgHeightHex, false]))
			.result;
		expect(ethBlockForkA).to.not.be.null;
		const ethHashForkA = ethBlockForkA.hash as string;
		// The parent of fork A's block at the reorg height is the anchor's Ethereum hash.
		// Fork B's block at the same height will share the same Ethereum parent.
		const anchorEthHash = ethBlockForkA.parentHash as string;

		// --- Fork B: 3 blocks (longer, wins the reorg) ---
		const b1 = await createBlock(false, anchor);
		const b2 = await createBlock(false, b1);
		await createBlock(false, b2);

		// Wait for fork B to become canonical (longer chain).
		const expectedHead = "0x" + (tipNumber + 4).toString(16);
		await waitForBlock(context.web3, expectedHead, 20_000);

		// After the reorg, the Ethereum block at the reorg height must differ.
		const ethBlockAfterReorg = (await customRequest(context.web3, "eth_getBlockByNumber", [reorgHeightHex, false]))
			.result;

		expect(ethBlockAfterReorg).to.not.be.null;
		expect(ethBlockAfterReorg.hash).to.not.equal(
			ethHashForkA,
			"block hash at the reorg height must change to fork B's block"
		);
		// Both forks share the same parent (the anchor), so parentHash must be
		// the anchor's Ethereum hash.
		expect(ethBlockAfterReorg.parentHash).to.equal(
			anchorEthHash,
			"parent hash must still reference the common ancestor after reorg"
		);
	});

	step("transactions from the losing fork should not reference the old block after a reorg", async function () {
		this.timeout(60_000);

		// Capture the current chain tip before forking.
		const tipNumber = Number(await context.web3.eth.getBlockNumber());

		// Create the fork anchor (non-finalized).
		const anchor = await createBlock(false);
		const a1Height = tipNumber + 2;

		const startNonce = Number(await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT));

		// Send a tx that will be included in fork A.
		const forkATxHash = await sendTransfer(startNonce, "0x1");

		// --- Fork A: include the tx in 1 block ---
		await createBlock(false, anchor);

		// Wait for fork A's block to be indexed so the tx is visible.
		const a1HeightHex = "0x" + a1Height.toString(16);
		await waitForBlock(context.web3, a1HeightHex, 10_000);

		// Capture the Ethereum block hash where our tx landed in fork A.
		const a1EthBlock = (await customRequest(context.web3, "eth_getBlockByNumber", [a1HeightHex, false])).result;
		const a1EthHash = a1EthBlock.hash as string;

		// Verify the tx is retrievable while fork A is canonical.
		const txBeforeReorg = (await customRequest(context.web3, "eth_getTransactionByHash", [forkATxHash])).result;
		expect(txBeforeReorg).to.not.be.null;
		expect(txBeforeReorg.hash).to.equal(forkATxHash);
		expect(txBeforeReorg.blockHash).to.equal(a1EthHash);

		// --- Fork B: 2 blocks (longer, no matching tx) ---
		const b1 = await createBlock(false, anchor);
		await createBlock(false, b1);

		// Wait for fork B to become canonical (longer chain).
		const expectedHead = "0x" + (tipNumber + 3).toString(16);
		await waitForBlock(context.web3, expectedHead, 20_000);

		// After the reorg, the block at fork A's height belongs to fork B.
		const blockAfterReorg = (await customRequest(context.web3, "eth_getBlockByNumber", [a1HeightHex, false]))
			.result;
		expect(blockAfterReorg).to.not.be.null;
		expect(blockAfterReorg.hash).to.not.equal(a1EthHash, "block at reorg height must be from fork B after reorg");

		// The fork A tx should either be null (reorged out and not re-included)
		// or have a different block hash (re-included in fork B). Since fork B
		// was built without the tx in the pool, it likely won't be re-included.
		const txAfterReorg = (await customRequest(context.web3, "eth_getTransactionByHash", [forkATxHash])).result;
		if (txAfterReorg !== null) {
			expect(txAfterReorg.blockHash).to.not.equal(
				a1EthHash,
				"reorged tx must not reference the losing fork's Ethereum block hash"
			);
		}
	});
});
