import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { customRequest, describeWithFrontierWs, waitForReceipt } from "./util";

const TRANSFER_TOPIC = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const LOG_EMITTING_CONSTRUCTOR =
	"0x" +
	"7f" +
	"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" +
	"600052" +
	"7f" +
	TRANSFER_TOPIC.slice(2) +
	"60206000a1" +
	"60006000f3";

describeWithFrontierWs("Frontier RPC (Log Reorg Compliance)", (context) => {
	let subscription;

	async function sleep(ms: number) {
		await new Promise<void>((resolve) => setTimeout(resolve, ms));
	}

	async function createBlock(finalize: boolean = true, parentHash: string | null = null): Promise<string> {
		const response = await customRequest(context.web3, "engine_createBlock", [true, finalize, parentHash]);
		if (!response.result?.hash) {
			throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
		}
		await sleep(300);
		return response.result.hash as string;
	}

	async function sendLogTransaction() {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: LOG_EMITTING_CONSTRUCTOR,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x1000000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		return tx;
	}

	async function waitForMatchingEvent(events: any[], predicate: (event: any) => boolean, timeoutMs = 15000) {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			const match = events.find(predicate);
			if (match) {
				return match;
			}
			await sleep(100);
		}
		throw new Error("Timed out waiting for matching log event");
	}

	async function waitForFilterChange(filterId: string, predicate: (event: any) => boolean, timeoutMs = 15000) {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			const response = await customRequest(context.web3, "eth_getFilterChanges", [filterId]);
			const logs = (response.result || []) as any[];
			const match = logs.find(predicate);
			if (match) {
				return match;
			}
			await sleep(100);
		}
		throw new Error("Timed out waiting for matching filter change");
	}

	step("logs subscription should emit removed=true when a logged tx is reorged out", async function () {
		this.timeout(60000);

		subscription = context.web3.eth.subscribe("logs", {}, function (_error, _result) {});
		await new Promise<void>((resolve, reject) => {
			const timer = setTimeout(
				() => reject(new Error("Timed out waiting for logs subscription connection")),
				10000
			);
			subscription.on("connected", function () {
				clearTimeout(timer);
				resolve();
			});
			subscription.on("error", function (error: any) {
				clearTimeout(timer);
				reject(error);
			});
		});

		const events: any[] = [];
		subscription.on("data", function (event: any) {
			events.push(event);
		});

		const anchor = await createBlock(false);
		const tx = await sendLogTransaction();
		await createBlock(false, anchor);

		const receipt = await waitForReceipt(context.web3, tx.transactionHash);
		const firstEvent = await waitForMatchingEvent(
			events,
			(event) => event.transactionHash === tx.transactionHash && event.removed === false
		);
		expect(firstEvent.blockHash).to.equal(receipt.blockHash);

		const b1 = await createBlock(false, anchor);
		await createBlock(false, b1);

		const removedEvent = await waitForMatchingEvent(
			events,
			(event) => event.transactionHash === tx.transactionHash && event.removed === true
		);
		expect(removedEvent.blockHash).to.equal(receipt.blockHash);

		subscription.unsubscribe();
	}).timeout(60000);

	step("eth_getFilterChanges should emit removed=true when a logged tx is reorged out", async function () {
		this.timeout(60000);

		const filterId = (await customRequest(context.web3, "eth_newFilter", [{}])).result as string;
		const anchor = await createBlock(false);
		const tx = await sendLogTransaction();
		await createBlock(false, anchor);

		const receipt = await waitForReceipt(context.web3, tx.transactionHash);
		const firstEvent = await waitForFilterChange(
			filterId,
			(event) => event.transactionHash === tx.transactionHash && event.removed === false
		);
		expect(firstEvent.blockHash).to.equal(receipt.blockHash);

		const b1 = await createBlock(false, anchor);
		await createBlock(false, b1);

		const removedEvent = await waitForFilterChange(
			filterId,
			(event) => event.transactionHash === tx.transactionHash && event.removed === true
		);
		expect(removedEvent.blockHash).to.equal(receipt.blockHash);
		expect(removedEvent.topics[0]).to.equal(TRANSFER_TOPIC);
	}).timeout(60000);
});
