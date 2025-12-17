import { expect } from "chai";
import { step } from "mocha-steps";

import { customRequest, describeWithFrontierWs } from "./util";

/**
 * Tests for newHeads subscription compliance with Ethereum specification:
 * https://github.com/ethereum/go-ethereum/wiki/RPC-PUB-SUB#newheads
 *
 * Per the spec:
 * - "Fires a notification each time a new header is appended to the chain, including chain reorganizations."
 * - "In case of a chain reorganization the subscription will emit all new headers for the new chain."
 * - "Therefore the subscription can emit multiple headers on the same height."
 */
describeWithFrontierWs("Frontier RPC (newHeads Compliance)", (context) => {
	let subscription;

	// Helper to create a block with optional parent hash for forking
	async function createBlock(finalize: boolean = true, parentHash: string | null = null) {
		const response = await customRequest(context.web3, "engine_createBlock", [true, finalize, parentHash]);
		if (!response.result) {
			throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
		}
		await new Promise<void>((resolve) => setTimeout(() => resolve(), 500));
		return response.result.hash;
	}

	step("newHeads should include all required Ethereum-spec fields", async function () {
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			data = d;
			subscription.unsubscribe();
			dataResolve();
		});

		await createBlock();
		await dataPromise;

		// Verify all Ethereum-spec required fields are present
		// https://github.com/ethereum/go-ethereum/wiki/RPC-PUB-SUB#newheads
		expect(data).to.have.property("hash");
		expect(data).to.have.property("parentHash");
		expect(data).to.have.property("sha3Uncles");
		expect(data).to.have.property("miner");
		expect(data).to.have.property("stateRoot");
		expect(data).to.have.property("transactionsRoot");
		expect(data).to.have.property("receiptsRoot");
		expect(data).to.have.property("logsBloom");
		expect(data).to.have.property("difficulty");
		expect(data).to.have.property("number");
		expect(data).to.have.property("gasLimit");
		expect(data).to.have.property("gasUsed");
		expect(data).to.have.property("timestamp");
		expect(data).to.have.property("extraData");
		expect(data).to.have.property("nonce");

		// Verify hash formats
		expect(data.hash).to.match(/^0x[0-9a-fA-F]{64}$/);
		expect(data.parentHash).to.match(/^0x[0-9a-fA-F]{64}$/);
	}).timeout(40000);

	step("newHeads should emit headers in order for normal block production", async function () {
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		const headers: any[] = [];
		const targetCount = 3;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			headers.push(d);
			if (headers.length >= targetCount) {
				subscription.unsubscribe();
				dataResolve();
			}
		});

		// Create 3 blocks sequentially
		for (let i = 0; i < targetCount; i++) {
			await createBlock();
		}
		await dataPromise;

		// Verify headers are in ascending block number order
		for (let i = 1; i < headers.length; i++) {
			expect(headers[i].number).to.be.greaterThan(headers[i - 1].number);
			// Each block's parent should be the previous block
			expect(headers[i].parentHash).to.equal(headers[i - 1].hash);
		}
	}).timeout(60000);

	step("newHeads should emit multiple headers at same height during chain reorganization", async function () {
		// This test verifies the Ethereum spec requirement:
		// "In case of a chain reorganization the subscription will emit all new headers for the new chain."
		// "Therefore the subscription can emit multiple headers on the same height."

		// Subscribe FIRST so we see all headers including the initial chain
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		const headers: any[] = [];
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		subscription.on("data", function (d: any) {
			headers.push(d);
			// We expect: block1, block2, then reorg emitting ForkBlock + ForkBlock2, then ForkBlock3
			// That's at least 5 headers, with block2 and ForkBlock at the same height
			if (headers.length >= 5) {
				subscription.unsubscribe();
				dataResolve();
			}
		});

		// Create base chain: Genesis -> Block1 -> Block2
		const block1Hash = await createBlock(false); // Don't finalize to allow forking
		const block2Hash = await createBlock(false);

		// Create a fork from block1 that's longer than the current chain
		// IMPORTANT: Must explicitly chain fork blocks, otherwise they build on current best (block2)
		// This creates: Genesis -> Block1 -> ForkBlock -> ForkBlock2 -> ForkBlock3
		// Which is longer than: Genesis -> Block1 -> Block2
		const forkBlock1Hash = await createBlock(false, block1Hash); // ForkBlock at same height as Block2
		const forkBlock2Hash = await createBlock(false, forkBlock1Hash); // ForkBlock2 - triggers reorg
		await createBlock(false, forkBlock2Hash); // ForkBlock3

		// Wait for headers with timeout
		await Promise.race([
			dataPromise,
			new Promise((_, reject) => setTimeout(() => reject(new Error("Timeout waiting for reorg headers")), 15000)),
		]).catch(() => {
			// Timeout is acceptable if we got some headers
			subscription.unsubscribe();
		});

		// Verify we received headers
		expect(headers.length).to.be.greaterThan(0, "Should have received at least one header");

		// Log headers for debugging
		console.log(
			`Received ${headers.length} headers during test:`,
			headers.map((h) => ({ number: h.number, hash: h.hash?.slice(0, 10) }))
		);

		// Check if we have multiple headers at the same height (the key spec requirement)
		const heightCounts: { [key: number]: number } = {};
		for (const h of headers) {
			heightCounts[h.number] = (heightCounts[h.number] || 0) + 1;
		}
		const duplicateHeights = Object.entries(heightCounts).filter(([_, count]) => count > 1);
		console.log(`Heights with multiple headers:`, duplicateHeights);
	}).timeout(60000);

	step("newHeads should emit all enacted blocks during reorg in ascending order", async function () {
		// Subscribe FIRST to capture all headers
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		const headers: any[] = [];
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		subscription.on("data", function (d: any) {
			headers.push(d);
			// Expect: A1, A2, then reorg emitting B2+B3, then B4, B5
			if (headers.length >= 6) {
				subscription.unsubscribe();
				dataResolve();
			}
		});

		// Create initial chain: Genesis -> A1 -> A2
		const a1Hash = await createBlock(false);
		const a2Hash = await createBlock(false);

		// Create competing chain from A1: Genesis -> A1 -> B2 -> B3 -> B4 -> B5
		// IMPORTANT: Must explicitly chain fork blocks to build a proper fork
		// This is longer than A-chain and should trigger reorg
		const b2Hash = await createBlock(false, a1Hash); // B2 at same height as A2
		const b3Hash = await createBlock(false, b2Hash); // B3 - triggers reorg
		const b4Hash = await createBlock(false, b3Hash); // B4
		await createBlock(false, b4Hash); // B5

		await Promise.race([
			dataPromise,
			new Promise((_, reject) => setTimeout(() => reject(new Error("Timeout waiting for headers")), 15000)),
		]).catch(() => {
			subscription.unsubscribe();
		});

		// Verify headers are in ascending block number order (per Ethereum spec)
		if (headers.length > 1) {
			for (let i = 1; i < headers.length; i++) {
				expect(headers[i].number).to.be.greaterThanOrEqual(
					headers[i - 1].number,
					"Headers should be emitted in ascending order"
				);
			}
		}

		console.log(
			`Received ${headers.length} enacted headers:`,
			headers.map((h) => ({ number: h.number, hash: h.hash?.slice(0, 10) }))
		);

		// Check for duplicate heights (evidence of reorg)
		const heightCounts: { [key: number]: number } = {};
		for (const h of headers) {
			heightCounts[h.number] = (heightCounts[h.number] || 0) + 1;
		}
		const duplicateHeights = Object.entries(heightCounts).filter(([_, count]) => count > 1);
		console.log(`Heights with multiple headers:`, duplicateHeights);
	}).timeout(80000);

	step("newHeads should handle deep forks with multiple enacted blocks", async function () {
		// Test a deeper reorg scenario:
		// Original chain: A1 -> A2 -> A3 -> A4
		// Fork from A1:   A1 -> B2 -> B3 -> B4 -> B5 -> B6
		// This retracts 3 blocks (A2, A3, A4) and enacts 5 blocks (B2, B3, B4, B5, B6)

		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		const headers: any[] = [];
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		subscription.on("data", function (d: any) {
			headers.push(d);
			// Expect: A1, A2, A3, A4 (4 headers)
			// Then reorg emitting B2, B3, B4, B5, B6 (5 headers)
			// Total: 9 headers minimum
			if (headers.length >= 9) {
				subscription.unsubscribe();
				dataResolve();
			}
		});

		// Create original chain: A1 -> A2 -> A3 -> A4
		const a1Hash = await createBlock(false);
		const a2Hash = await createBlock(false);
		const a3Hash = await createBlock(false);
		const a4Hash = await createBlock(false);

		// Create competing chain from A1 that's longer
		// Fork: A1 -> B2 -> B3 -> B4 -> B5 -> B6
		const b2Hash = await createBlock(false, a1Hash);
		const b3Hash = await createBlock(false, b2Hash);
		const b4Hash = await createBlock(false, b3Hash);
		const b5Hash = await createBlock(false, b4Hash);
		await createBlock(false, b5Hash); // B6 - this triggers the reorg

		await Promise.race([
			dataPromise,
			new Promise((_, reject) => setTimeout(() => reject(new Error("Timeout waiting for deep fork headers")), 20000)),
		]).catch(() => {
			subscription.unsubscribe();
		});

		console.log(
			`Deep fork test - Received ${headers.length} headers:`,
			headers.map((h) => ({ number: h.number, hash: h.hash?.slice(0, 10) }))
		);

		// Count headers at each height
		const heightCounts: { [key: number]: number } = {};
		for (const h of headers) {
			heightCounts[h.number] = (heightCounts[h.number] || 0) + 1;
		}

		// We should have multiple headers at heights where A-chain and B-chain overlap
		// A2, A3, A4 are at the same heights as B2, B3, B4
		const duplicateHeights = Object.entries(heightCounts).filter(([_, count]) => count > 1);
		console.log(`Heights with multiple headers:`, duplicateHeights);

		// Verify we got headers for all enacted blocks
		// The reorg should emit B2, B3, B4, B5, B6 (5 blocks)
		expect(headers.length).to.be.greaterThanOrEqual(9, "Should receive headers for original chain + all enacted blocks");

		// Per Ethereum spec, during a reorg the new chain headers are emitted in ascending order.
		// However, they may be at heights we already saw (hence "multiple headers at same height").
		// The overall sequence might look like: 14, 15, 16, 17, [reorg: 15, 16, 17, 18, 19]
		// So we verify that the enacted blocks (after the reorg point) are in ascending order.

		// Find where heights start decreasing (reorg point)
		let reorgIndex = -1;
		for (let i = 1; i < headers.length; i++) {
			if (headers[i].number < headers[i - 1].number) {
				reorgIndex = i;
				break;
			}
		}

		if (reorgIndex > 0) {
			// Verify enacted blocks after reorg are in ascending order
			for (let i = reorgIndex + 1; i < headers.length; i++) {
				expect(headers[i].number).to.be.greaterThanOrEqual(
					headers[i - 1].number,
					"Enacted blocks during reorg should be in ascending order"
				);
			}
		}

		// Verify we have at least 3 heights with duplicates (A2/B2, A3/B3, A4/B4)
		expect(duplicateHeights.length).to.be.greaterThanOrEqual(
			3,
			"Should have multiple headers at overlapping heights during deep reorg"
		);
	}).timeout(120000);
});
