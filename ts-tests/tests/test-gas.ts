import { expect } from "chai";
import { ethers } from "ethers";
import { step } from "mocha-steps";
import { AbiItem } from "web3-utils";

import InvalidOpcode from "../build/contracts/InvalidOpcode.json";
import Test from "../build/contracts/Test.json";
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

describeWithFrontier("Frontier RPC (Gas limit Weightv2 ref time)", (context) => {
	step("gas limit bound works with ref time heavy txns", async function () {
		this.timeout(100000000);

		let transfers_per_block = Math.round(ETH_BLOCK_GAS_LIMIT / 21_000);
		// do transfer transfers_per_block + 1
		for (var nonce = 0; nonce < transfers_per_block + 1; nonce++) {
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: TEST_ACCOUNT,
					value: "0x1",
					gasPrice: "0x3B9ACA00",
					gas: "0x5208",
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		}

		await createAndFinalizeBlock(context.web3);

		let latest = await context.web3.eth.getBlock("latest");
		expect(latest.transactions.length).to.be.eq(transfers_per_block);
		expect(latest.gasUsed).to.be.lessThanOrEqual(ETH_BLOCK_GAS_LIMIT);
		expect(ETH_BLOCK_GAS_LIMIT - latest.gasUsed).to.be.lessThan(21_000);
	});
});

describeWithFrontier("Frontier RPC (Gas limit Weightv2 pov size)", (context) => {
	// This test fills a block with regular transfers + a transfer to a contract with big bytecode.
	// We consider bytecode "big" when it consumes an effective gas greater than the legacy gas.
	step("gas limit bound works with pov size heavy txns", async function () {
		this.timeout(100000000);
		let nonce = 0;
		let tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_ERC20_BYTECODE,
				gas: "0x1000000",
				gasPrice: "0x3B9ACA00",
				nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const { result } = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		const receipt = await context.web3.eth.getTransactionReceipt(result);
		let contractAddress = receipt.contractAddress;

		let contract_byte_size = (await (await context.web3.eth.getCode(contractAddress)).length) / 2;

		let gas_per_byte = Math.round(ETH_BLOCK_GAS_LIMIT / ETH_BLOCK_POV_LIMIT); // 75_000_000 / 5_242_880
		let contract_transfer_effective_gas = contract_byte_size * gas_per_byte; // 100_520

		// Transfer to contract
		nonce += 1;
		tx = await context.web3.eth.accounts.signTransaction(
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

		// Fill the rest of the block with regular transfers
		let transfers_per_block = Math.floor((ETH_BLOCK_GAS_LIMIT - contract_transfer_effective_gas) / 21_000);
		// do transfer transfers_per_block + some number
		for (var i = 0; i < transfers_per_block + 100; i++) {
			nonce += 1;
			tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: TEST_ACCOUNT,
					value: "0x1",
					gasPrice: "0x3B9ACA00",
					gas: "0x5208",
					nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		}

		await createAndFinalizeBlock(context.web3);

		let latest = await context.web3.eth.getBlock("latest");
		// Expect all regular transfers to go through + contract transfer.
		expect(latest.transactions.length).to.be.eq(transfers_per_block + 1);
		expect(latest.transactions).contain(contract_transfer_hash);
		expect(latest.gasUsed).to.be.lessThanOrEqual(ETH_BLOCK_GAS_LIMIT);
		expect(ETH_BLOCK_GAS_LIMIT - latest.gasUsed).to.be.lessThan(21_000);
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
