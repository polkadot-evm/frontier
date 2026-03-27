import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontierWs, waitForReceipt } from "./util";

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

const TEST_CONTRACT_BYTECODE =
	"0x608060405234801561001057600080fd5b50610041337fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff61004660201b60201c565b610291565b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff1614156100e9576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601f8152602001807f45524332303a206d696e7420746f20746865207a65726f20616464726573730081525060200191505060405180910390fd5b6101028160025461020960201b610c7c1790919060201c565b60028190555061015d816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205461020960201b610c7c1790919060201c565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff16600073ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a35050565b600080828401905083811015610287576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b8091505092915050565b610e3a806102a06000396000f3fe608060405234801561001057600080fd5b50600436106100885760003560e01c806370a082311161005b57806370a08231146101fd578063a457c2d714610255578063a9059cbb146102bb578063dd62ed3e1461032157610088565b8063095ea7b31461008d57806318160ddd146100f357806323b872dd146101115780633950935114610197575b600080fd5b6100d9600480360360408110156100a357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610399565b604051808215151515815260200191505060405180910390f35b6100fb6103b7565b6040518082815260200191505060405180910390f35b61017d6004803603606081101561012757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803590602001909291905050506103c1565b604051808215151515815260200191505060405180910390f35b6101e3600480360360408110156101ad57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff1690602001909291908035906020019092919050505061049a565b604051808215151515815260200191505060405180910390f35b61023f6004803603602081101561021357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919050505061054d565b6040518082815260200191505060405180910390f35b6102a16004803603604081101561026b57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610595565b604051808215151515815260200191505060405180910390f35b610307600480360360408110156102d157600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610662565b604051808215151515815260200191505060405180910390f35b6103836004803603604081101561033757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190505050610680565b6040518082815260200191505060405180910390f35b60006103ad6103a6610707565b848461070f565b6001905092915050565b6000600254905090565b60006103ce848484610906565b61048f846103da610707565b61048a85604051806060016040528060288152602001610d7060289139600160008b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000206000610440610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b600190509392505050565b60006105436104a7610707565b8461053e85600160006104b8610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008973ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b61070f565b6001905092915050565b60008060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020549050919050565b60006106586105a2610707565b8461065385604051806060016040528060258152602001610de160259139600160006105cc610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008a73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b6001905092915050565b600061067661066f610707565b8484610906565b6001905092915050565b6000600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054905092915050565b600033905090565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff161415610795576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526024815260200180610dbd6024913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff16141561081b576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526022815260200180610d286022913960400191505060405180910390fd5b80600160008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925836040518082815260200191505060405180910390a3505050565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff16141561098c576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526025815260200180610d986025913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff161415610a12576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526023815260200180610d056023913960400191505060405180910390fd5b610a7d81604051806060016040528060268152602001610d4a602691396000808773ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b6000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002081905550610b10816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a3505050565b6000838311158290610c69576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825283818151815260200191508051906020019080838360005b83811015610c2e578082015181840152602081019050610c13565b50505050905090810190601f168015610c5b5780820380516001836020036101000a031916815260200191505b509250505060405180910390fd5b5060008385039050809150509392505050565b600080828401905083811015610cfa576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b809150509291505056fe45524332303a207472616e7366657220746f20746865207a65726f206164647265737345524332303a20617070726f766520746f20746865207a65726f206164647265737345524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e636545524332303a207472616e7366657220616d6f756e74206578636565647320616c6c6f77616e636545524332303a207472616e736665722066726f6d20746865207a65726f206164647265737345524332303a20617070726f76652066726f6d20746865207a65726f206164647265737345524332303a2064656372656173656420616c6c6f77616e63652062656c6f77207a65726fa265627a7a72315820c7a5ffabf642bda14700b2de42f8c57b36621af020441df825de45fd2b3e1c5c64736f6c63430005100032";

/**
 * Reorg log semantics: invalidated logs appear with removed: true (filters + pub/sub).
 * Note: web3.js v1 emits log tombstones on subscription "changed", not "data".
 */
describeWithFrontierWs("Frontier RPC (Log Reorg Compliance)", (context) => {
	let subscription;

	async function sleep(ms: number) {
		await new Promise<void>((resolve) => setTimeout(resolve, ms));
	}

	async function waitForSubscriptionConnection(subscription: any) {
		return new Promise<void>((resolve, reject) => {
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

		const sendRes = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		const txHash = (sendRes.result as string) || (tx.transactionHash as string);
		return { ...tx, transactionHash: txHash };
	}

	async function deployErc20ContractTx() {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x1000000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const sendRes = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		const txHash = (sendRes.result as string) || (tx.transactionHash as string);
		return { ...tx, transactionHash: txHash };
	}

	async function waitForMatchingEvent(events: any[], predicate: (event: any) => boolean, timeoutMs = 60000) {
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

	async function waitForFilterChange(filterId: string, predicate: (event: any) => boolean, timeoutMs = 60000) {
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

		const events: any[] = [];
		// web3.js emits log tombstones (removed: true) on "changed", not "data" (see web3-eth subscribe logs subscriptionHandler).
		const recordLog = (event: any) => {
			events.push(event);
		};
		subscription.on("data", recordLog);
		subscription.on("changed", recordLog);
		await waitForSubscriptionConnection(subscription);

		const anchor = await createAndFinalizeBlock(context.web3, false);
		const tx = await sendLogTransaction();
		await createAndFinalizeBlock(context.web3, false, anchor);

		const receipt = await waitForReceipt(context.web3, tx.transactionHash);
		const firstEvent = await waitForMatchingEvent(
			events,
			(event) =>
				event.transactionHash?.toLowerCase() === (tx.transactionHash as string).toLowerCase() &&
				event.removed !== true
		);
		expect(firstEvent.blockHash).to.equal(receipt.blockHash);

		const b1 = await createAndFinalizeBlock(context.web3, false, anchor, true);
		await createAndFinalizeBlock(context.web3, false, b1, true);

		const removedEvent = await waitForMatchingEvent(
			events,
			(event) =>
				event.transactionHash?.toLowerCase() === (tx.transactionHash as string).toLowerCase() &&
				event.removed === true
		);
		expect(removedEvent.blockHash).to.equal(receipt.blockHash);

		const winningTx = await sendLogTransaction();
		await createAndFinalizeBlock(context.web3, false);
		await waitForReceipt(context.web3, winningTx.transactionHash as string);
		const canonicalAfterReorg = await waitForMatchingEvent(
			events,
			(e) =>
				e.transactionHash?.toLowerCase() === (winningTx.transactionHash as string).toLowerCase() &&
				e.removed !== true
		);
		expect(canonicalAfterReorg.removed, "winning-branch log must not be a tombstone").to.not.equal(true);

		subscription.unsubscribe();
	}).timeout(60000);

	step("eth_getFilterChanges should emit removed=true when a logged tx is reorged out", async function () {
		this.timeout(60000);

		const filterId = (await customRequest(context.web3, "eth_newFilter", [{}])).result as string;
		const anchor = await createAndFinalizeBlock(context.web3, false);
		const tx = await sendLogTransaction();
		await createAndFinalizeBlock(context.web3, false, anchor);

		const receipt = await waitForReceipt(context.web3, tx.transactionHash);
		const firstEvent = await waitForFilterChange(
			filterId,
			(event) =>
				event.transactionHash?.toLowerCase() === (tx.transactionHash as string).toLowerCase() &&
				event.removed !== true
		);
		expect(firstEvent.blockHash).to.equal(receipt.blockHash);

		const b1 = await createAndFinalizeBlock(context.web3, false, anchor, true);
		await createAndFinalizeBlock(context.web3, false, b1, true);

		const removedEvent = await waitForFilterChange(
			filterId,
			(event) =>
				event.transactionHash?.toLowerCase() === (tx.transactionHash as string).toLowerCase() &&
				event.removed === true
		);
		expect(removedEvent.blockHash).to.equal(receipt.blockHash);
		expect(removedEvent.topics[0]).to.equal(TRANSFER_TOPIC);

		const winningTx = await sendLogTransaction();
		await createAndFinalizeBlock(context.web3, false);
		await waitForReceipt(context.web3, winningTx.transactionHash as string);
		const canonicalAfterReorg = await waitForFilterChange(
			filterId,
			(e) =>
				e.transactionHash?.toLowerCase() === (winningTx.transactionHash as string).toLowerCase() &&
				e.removed !== true
		);
		expect(canonicalAfterReorg.removed === true, "winning-branch log must not be a tombstone").to.equal(false);
		await customRequest(context.web3, "eth_uninstallFilter", [filterId]);
	}).timeout(60000);

	step("logs subscription should emit removed=true after a longer fork (ERC20 deploy)", async function () {
		subscription = context.web3.eth.subscribe("logs", {}, () => {});

		const logEvents: any[] = [];
		const recordLog = (d: any) => {
			logEvents.push(d);
		};
		subscription.on("data", recordLog);
		subscription.on("changed", recordLog);
		await waitForSubscriptionConnection(subscription);

		// Canonical chain: A1 -> A2 (includes deploy tx + log)
		const a1Hash = await createAndFinalizeBlock(context.web3, false);
		const signedTx = await deployErc20ContractTx();
		const txHash = signedTx.transactionHash as string;
		await createAndFinalizeBlock(context.web3, false);

		const receipt = await waitForReceipt(context.web3, txHash, 20000);
		const blockWithDeploy = await context.web3.eth.getBlock(receipt.blockNumber, false);
		expect(blockWithDeploy).to.not.be.null;
		const retractedEthBlockHash = blockWithDeploy!.hash as string;

		// Wait for the canonical log event before triggering reorg
		await waitForMatchingEvent(
			logEvents,
			(e) => e.transactionHash?.toLowerCase() === txHash.toLowerCase() && e.removed !== true,
			10000
		);

		// Longer fork from A1: A1 -> B2 -> B3 (retracts the block that contained the deploy)
		const b2Hash = await createAndFinalizeBlock(context.web3, false, a1Hash, true);
		await createAndFinalizeBlock(context.web3, false, b2Hash, true);

		const pollDeadline = Date.now() + 60000;
		while (Date.now() < pollDeadline) {
			if (logEvents.some((e) => e.removed === true)) {
				break;
			}
			await sleep(300);
		}

		const postForkTx = await sendLogTransaction();
		await createAndFinalizeBlock(context.web3, false);
		await waitForReceipt(context.web3, postForkTx.transactionHash as string);
		const canonicalOnWinningFork = await waitForMatchingEvent(
			logEvents,
			(e) =>
				e.transactionHash?.toLowerCase() === (postForkTx.transactionHash as string).toLowerCase() &&
				e.removed !== true
		);
		expect(canonicalOnWinningFork.removed === true, "winning-branch log must not be a tombstone").to.equal(false);

		subscription.unsubscribe();

		const removedTrue = logEvents.filter((e) => e.removed === true);
		const canonicalEvents = logEvents.filter((e) => e.removed !== true);

		expect(canonicalEvents.length, "should see at least one canonical log before/during test").to.be.at.least(1);

		expect(
			removedTrue.length,
			`expected removed=true after reorg; events: ${JSON.stringify(
				logEvents.map((e) => ({ removed: e.removed, blockHash: e.blockHash?.slice(0, 12) }))
			)}`
		).to.be.at.least(1);

		const matchesRetractedBlock = removedTrue.some(
			(e) => e.blockHash?.toLowerCase() === retractedEthBlockHash.toLowerCase()
		);
		expect(matchesRetractedBlock, "removed=true log should reference the retracted Ethereum block hash").to.equal(
			true
		);
	}).timeout(120000);
});
