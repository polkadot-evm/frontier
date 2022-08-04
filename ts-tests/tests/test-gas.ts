import { expect } from "chai";
import { AbiItem } from "web3-utils";

import Test from "../build/contracts/Test.json";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, FIRST_CONTRACT_ADDRESS } from "./config";
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

	// EXTRINSIC_GAS_LIMIT = [BLOCK_GAS_LIMIT - BLOCK_GAS_LIMIT * (NORMAL_DISPATCH_RATIO - AVERAGE_ON_INITIALIZE_RATIO) - EXTRINSIC_BASE_Weight] / WEIGHT_PER_GAS = (1_000_000_000_000 * 2 * (0.75-0.1) - 125_000_000) / 20000
	const EXTRINSIC_GAS_LIMIT = 64995685;

	it("eth_estimateGas for contract creation", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let oneOffEstimation = 196657;
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
		let oneOffEstimation = 196657 + 4300;
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
		expect(result).to.equal(197690);
		result = await context.web3.eth.estimateGas({
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
		});
		expect(result).to.equal(197690);
	});

	it("tx gas limit below EXTRINSIC_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: EXTRINSIC_GAS_LIMIT - 1,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).transactionHash).to.be.not.null;
		expect((createReceipt as any).blockHash).to.be.not.null;
	});
	it("tx gas limit equal EXTRINSIC_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: EXTRINSIC_GAS_LIMIT,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).transactionHash).to.be.not.null;
		expect((createReceipt as any).blockHash).to.be.not.null;
	});
	it("tx gas limit larger EXTRINSIC_GAS_LIMIT", async function () {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
				gas: EXTRINSIC_GAS_LIMIT + 1,
				gasPrice: "0x3B9ACA00",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const createReceipt = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		expect((createReceipt as any).error.message).to.equal(
			"submit transaction to pool failed: Pool(InvalidTransaction(InvalidTransaction::ExhaustsResources))"
		);
	});
});
