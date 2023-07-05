import { expect } from "chai";
import { AbiItem } from "web3-utils";

import InvalidOpcode from "../build/contracts/InvalidOpcode.json";
import Test from "../build/contracts/Test.json";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, FIRST_CONTRACT_ADDRESS, ETH_BLOCK_GAS_LIMIT } from "./config";
import { describeWithFrontier, createAndFinalizeBlock, customRequest } from "./util";

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
		let oneOffEstimation = 196701;
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

	it.skip("block gas limit over 5M", async function () {
		expect((await context.web3.eth.getBlock("latest")).gasLimit).to.be.above(5000000);
	});

	// Testing the gas limit protection, hardcoded to 25M
	it.skip("gas limit should decrease on next block if gas unused", async function () {
		this.timeout(15000);

		const gasLimit = (await context.web3.eth.getBlock("latest")).gasLimit;
		await createAndFinalizeBlock(context.web3);

		// Gas limit is expected to have decreased as the gasUsed by the block is lower than 2/3 of the previous gas limit
		const newGasLimit = (await context.web3.eth.getBlock("latest")).gasLimit;
		expect(newGasLimit).to.be.below(gasLimit);
	});

	// Testing the gas limit protection, hardcoded to 25M
	it.skip("gas limit should increase on next block if gas fully used", async function () {
		// TODO: fill a block with many heavy transaction to simulate lot of gas.
	});

	it("eth_estimateGas for contract call", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let oneOffEstimation = 21204;
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
		let oneOffEstimation = 21204;
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
		let oneOffEstimation = 196701 + 4300;
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
		expect(result).to.equal(197732);
		result = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
		});
		expect(result).to.equal(197732);
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
		expect(estimate).to.equal(85703);
	});
});
