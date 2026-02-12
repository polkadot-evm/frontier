import { expect } from "chai";
import { ethers } from "ethers";
import { step } from "mocha-steps";
import { AbiItem } from "web3-utils";

import InvalidOpcode from "../build/contracts/InvalidOpcode.json";
import Test from "../build/contracts/Test.json";
import StorageLoop from "../build/contracts/StorageLoop.json";
import Web3 from "web3";
import {
	GENESIS_ACCOUNT,
	GENESIS_ACCOUNT_PRIVATE_KEY,
	FIRST_CONTRACT_ADDRESS,
	ETH_BLOCK_GAS_LIMIT,
	ETH_BLOCK_POV_LIMIT,
	TEST_ERC20_BYTECODE,
} from "./config";
import { describeWithFrontier, createAndFinalizeBlock, customRequest } from "./util";

const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

function withGasBuffer(gasEstimate: number): number {
	// Keep a small headroom to avoid rejects when runtime weights shift slightly.
	return Math.ceil(gasEstimate * 1.05) + 5_000;
}

// (!) The implementation must match the one in the rpc handler.
// If the variation in the estimate is less than 10%,
// then the estimate is considered sufficiently accurate.
const ESTIMATION_VARIANCE = 10;
function binarySearch(oneOffEstimation) {
	let highest = 4_294_967_295; // max(u32)
	let lowest = 21000;
	let mid = Math.min(oneOffEstimation * 3, (highest + lowest) / 2);
	let previousHighest = highest;
	while (true) {
		if (mid >= oneOffEstimation) {
			highest = mid;
			if (((previousHighest - highest) * ESTIMATION_VARIANCE) / previousHighest < 1) {
				break;
			}
			previousHighest = highest;
		} else {
			lowest = mid;
		}
		mid = Math.floor((highest + lowest) / 2);
	}
	return highest;
}

function estimationVariance(binarySearchEstimation, oneOffEstimation) {
	return ((binarySearchEstimation - oneOffEstimation) * ESTIMATION_VARIANCE) / binarySearchEstimation;
}

describeWithFrontier("Frontier RPC (Gas)", (context) => {
	const TEST_CONTRACT_ABI = Test.abi as AbiItem[];

	// Those test are ordered. In general this should be avoided, but due to the time it takes
	// to spin up a frontier node, it saves a lot of time.

	it("eth_estimateGas for contract creation", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let oneOffEstimation = 189151;
		let binarySearchEstimation = binarySearch(oneOffEstimation);
		// Sanity check expect a variance of 10%.
		expect(estimationVariance(binarySearchEstimation, oneOffEstimation)).to.be.lessThan(1);
		expect(
			await context.web3.eth.estimateGas({
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
			})
		).to.equal(binarySearchEstimation);
	});

	it("eth_estimateGas for contract call", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let oneOffEstimation = 21507;
		let binarySearchEstimation = binarySearch(oneOffEstimation);
		// Sanity check expect a variance of 10%.
		expect(estimationVariance(binarySearchEstimation, oneOffEstimation)).to.be.lessThan(1);
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});

		expect(await contract.methods.multiply(3).estimateGas()).to.equal(binarySearchEstimation);
	});

	it("eth_estimateGas without gas_limit should pass", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let oneOffEstimation = 21507;
		let binarySearchEstimation = binarySearch(oneOffEstimation);
		// Sanity check expect a variance of 10%.
		expect(estimationVariance(binarySearchEstimation, oneOffEstimation)).to.be.lessThan(1);
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
		});

		expect(await contract.methods.multiply(3).estimateGas()).to.equal(binarySearchEstimation);
	});

	it("eth_estimateGas should handle AccessList alias", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		// 4300 == 1900 for one key and 2400 for one storage.
		let oneOffEstimation = 189151 + 4300;
		let binarySearchEstimation = binarySearch(oneOffEstimation);
		// Sanity check expect a variance of 10%.
		expect(estimationVariance(binarySearchEstimation, oneOffEstimation)).to.be.lessThan(1);
		let result = (
			await customRequest(context.web3, "eth_estimateGas", [
				{
					from: GENESIS_ACCOUNT,
					data: Test.bytecode,
					accessList: [
						{
							address: "0x0000000000000000000000000000000000000000",
							storageKeys: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
						},
					],
				},
			])
		).result;
		expect(result).to.equal(context.web3.utils.numberToHex(binarySearchEstimation));
	});

	it("eth_estimateGas 0x0 gasPrice is equivalent to not setting one", async function () {
		let result = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
			gasPrice: "0x0",
		});
		expect(result).to.equal(189631);
		result = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
		});
		expect(result).to.equal(189631);
	});

	it("eth_estimateGas should ignore nonce", async function () {
		let result = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
			nonce: 42, // Arbitrary nonce value
		});
		expect(result).to.equal(189631);
	});

	it("tx gas limit below ETH_BLOCK_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: ETH_BLOCK_GAS_LIMIT - 1,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).transactionHash).to.be.not.null;
		expect((createReceipt as any).blockHash).to.be.not.null;
	});
	it("tx gas limit equal ETH_BLOCK_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: ETH_BLOCK_GAS_LIMIT,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).transactionHash).to.be.not.null;
		expect((createReceipt as any).blockHash).to.be.not.null;
	});
	it("tx gas limit larger ETH_BLOCK_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: ETH_BLOCK_GAS_LIMIT + 1,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).error.message).to.equal("exceeds block gas limit");
	});
});

describeWithFrontier("Frontier RPC (Gas limit Weightv2 ref time)", (context) => {
	const STORAGE_LOOP_CONTRACT_BYTECODE = StorageLoop.bytecode;
	const STORAGE_LOOP_CONTRACT_ABI = StorageLoop.abi as AbiItem[];

	before("create the contract", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: STORAGE_LOOP_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
	});

	step("gas limit bound works with ref time heavy txns", async function () {
		this.timeout(10000);

		const contract = new context.web3.eth.Contract(STORAGE_LOOP_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});
		const firstCallEstimate = await contract.methods.storageLoop(1000, TEST_ACCOUNT, 0).estimateGas({
			from: GENESIS_ACCOUNT,
		});
		const followUpCallEstimate = await contract.methods.storageLoop(1000, TEST_ACCOUNT, 1).estimateGas({
			from: GENESIS_ACCOUNT,
		});
		const firstCallGasLimit = withGasBuffer(firstCallEstimate);
		const followUpCallGasLimit = withGasBuffer(followUpCallEstimate);

		const blockGasAfterFirstCall = ETH_BLOCK_GAS_LIMIT - firstCallEstimate;
		// Number of calls per block (+1 for first call estimate).
		const callsPerBlock = Math.floor(blockGasAfterFirstCall / followUpCallEstimate) + 1;
		// Available gas space after all calls.
		const remnant = Math.floor(blockGasAfterFirstCall - followUpCallEstimate * (callsPerBlock - 1));
		// Number of transfers that should fit in the remnant.
		const transfersPerBlock = Math.floor(remnant / 21_000);
		const extraTransfers = 5;

		let nonce = await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT);

		for (var i = 0; i < callsPerBlock; i++) {
			let data = contract.methods.storageLoop(1000, TEST_ACCOUNT, i);
			let tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: contract.options.address,
					data: data.encodeABI(),
					gasPrice: "0x3B9ACA00",
					gas: `0x${(i === 0 ? firstCallGasLimit : followUpCallGasLimit).toString(16)}`,
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
			nonce++;
		}
		// because we are using Math.floor for everything, at the end there is room for an additional
		// transfer.
		for (var i = 0; i < transfersPerBlock + extraTransfers; i++) {
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: "0x2111111111111111111111111111111111111111",
					value: "0x1",
					gasPrice: "0x3B9ACA00",
					gas: "0x5208",
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
			nonce++;
		}

		await createAndFinalizeBlock(context.web3);

		let latest = await context.web3.eth.getBlock("latest");
		expect(latest.transactions.length).to.be.greaterThan(0);
		expect(latest.gasUsed).to.be.lessThanOrEqual(ETH_BLOCK_GAS_LIMIT);
	});
});

describeWithFrontier("Frontier RPC (Gas limit Weightv2 pov size)", (context) => {
	const STORAGE_LOOP_CONTRACT_BYTECODE = StorageLoop.bytecode;
	const STORAGE_LOOP_CONTRACT_ABI = StorageLoop.abi as AbiItem[];

	// Effective gas for transferring to contract with large bytecode (pov_size impact)
	const CONTRACT_TRANSFER_EFFECTIVE_GAS = 221_000;

	let contractAddress;
	before("create the contract", async function () {
		const tx1 = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: STORAGE_LOOP_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx1.rawTransaction]);
		const tx2 = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_ERC20_BYTECODE,
				gas: "0x1000000",
				gasPrice: "0x3B9ACA00",
				nonce: 1,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const { result } = await customRequest(context.web3, "eth_sendRawTransaction", [tx2.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		const receipt = await context.web3.eth.getTransactionReceipt(result);
		contractAddress = receipt.contractAddress;
	});

	// This test fills a block with regular transfers + a transfer to a contract with big bytecode.
	// We consider bytecode "big" when it consumes an effective gas greater than the legacy gas.
	step("gas limit bound works with pov size heavy txns", async function () {
		this.timeout(10000);

		const contract = new context.web3.eth.Contract(STORAGE_LOOP_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});
		const firstCallEstimate = await contract.methods.storageLoop(1000, TEST_ACCOUNT, 0).estimateGas({
			from: GENESIS_ACCOUNT,
		});
		const followUpCallEstimate = await contract.methods.storageLoop(1000, TEST_ACCOUNT, 1).estimateGas({
			from: GENESIS_ACCOUNT,
		});
		const followUpCallGasLimit = withGasBuffer(followUpCallEstimate);

		const blockGasAfterHeavyTx = ETH_BLOCK_GAS_LIMIT - (firstCallEstimate + CONTRACT_TRANSFER_EFFECTIVE_GAS);
		// Number of calls per block (+1 for first call estimate).
		const callsPerBlock = Math.floor(blockGasAfterHeavyTx / followUpCallEstimate) + 1;
		// Available gas space left after all calls.
		const remnant = Math.floor(blockGasAfterHeavyTx - followUpCallEstimate * (callsPerBlock - 1));
		// Number of transfers per available space left (+1 for the heavy transfer).
		const transfersPerBlock = Math.floor(remnant / 21_000) + 1;
		const extraTransfers = 5;

		let nonce = await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT);
		let tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: contractAddress,
				value: "0x1",
				gasPrice: "0x3B9ACA00",
				gas: "0xF4240",
				nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		let contract_transfer_hash = await (
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])
		).result;
		nonce++;

		for (var i = 0; i < callsPerBlock; i++) {
			let data = contract.methods.storageLoop(1000, TEST_ACCOUNT, i);
			let tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: contract.options.address,
					data: data.encodeABI(),
					gasPrice: "0x3B9ACA00",
					gas: `0x${followUpCallGasLimit.toString(16)}`,
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
			nonce++;
		}
		// because we are using Math.floor for everything, at the end there is room for an additional
		// transfer.
		for (var i = 0; i < transfersPerBlock + extraTransfers; i++) {
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: "0x2111111111111111111111111111111111111111",
					value: "0x1",
					gasPrice: "0x3B9ACA00",
					gas: "0x5208",
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
			nonce++;
		}

		await createAndFinalizeBlock(context.web3);

		let latest = await context.web3.eth.getBlock("latest");
		expect(latest.transactions.length).to.be.greaterThan(0);
		expect(contract_transfer_hash).to.be.a("string");
		expect(latest.gasUsed).to.be.lessThanOrEqual(ETH_BLOCK_GAS_LIMIT);

		// In slower CI environments the heavy transfer may be deferred to a following block.
		let receipt = await context.web3.eth.getTransactionReceipt(contract_transfer_hash);
		for (let i = 0; i < 3 && !receipt; i++) {
			await createAndFinalizeBlock(context.web3);
			receipt = await context.web3.eth.getTransactionReceipt(contract_transfer_hash);
		}
		expect(receipt, "expected heavy transfer to be mined within 4 sealed blocks").to.not.be.null;
		const minedBlock = await context.web3.eth.getBlock(receipt.blockNumber);
		expect(minedBlock.gasUsed).to.be.lessThanOrEqual(ETH_BLOCK_GAS_LIMIT);
	});
});

describeWithFrontier("Frontier RPC (Invalid opcode estimate gas)", (context) => {
	const INVALID_OPCODE_BYTECODE = InvalidOpcode.bytecode;

	let contractAddess;
	before(async () => {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: INVALID_OPCODE_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;
		await createAndFinalizeBlock(context.web3);
		contractAddess = (await context.web3.eth.getTransactionReceipt(txHash)).contractAddress;
	});

	it("should estimate gas with invalid opcode", async function () {
		let estimate = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			to: contractAddess,
			data: "0x28b5e32b", // selector for the contract's `call` method
		});
		// The actual estimated value is irrelevant for this test purposes, we just want to verify that
		// the binary search is not interrupted when an InvalidCode is returned by the evm.
		expect(estimate).to.equal(85699);
	});
});
