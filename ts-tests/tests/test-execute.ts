import { assert, expect } from "chai";
import { step } from "mocha-steps";
import { BLOCK_GAS_LIMIT, GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";

import { describeWithFrontier, customRequest, createAndFinalizeBlock } from "./util";
import { AbiItem } from "web3-utils";

import Test from "../build/contracts/Test.json";
import ForceGasLimit from "../build/contracts/ForceGasLimit.json";

// EXTRINSIC_GAS_LIMIT = [BLOCK_GAS_LIMIT - BLOCK_GAS_LIMIT * (NORMAL_DISPATCH_RATIO - AVERAGE_ON_INITIALIZE_RATIO) - EXTRINSIC_BASE_Weight] / WEIGHT_PER_GAS = (1_000_000_000_000 * 2 * (0.75-0.1) - 125_000_000) / 20000
const EXTRINSIC_GAS_LIMIT = 64995685;
const TEST_CONTRACT_BYTECODE = Test.bytecode;
const TEST_CONTRACT_DEPLOYED_BYTECODE = Test.deployedBytecode;

const FORCE_GAS_CONTRACT_BYTECODE = ForceGasLimit.bytecode;
const FORCE_GAS_CONTRACT_ABI = ForceGasLimit.abi as AbiItem[];
const FORCE_GAS_CONTRACT_DEPLOYED_BYTECODE = ForceGasLimit.deployedBytecode;

describeWithFrontier("Frontier RPC (RPC execution)", (context) => {
	step("should call with gas limit under block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${BLOCK_GAS_LIMIT.toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("should call with gas limit up to 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(BLOCK_GAS_LIMIT * 10).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("shouldn't call with gas limit up higher than 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(BLOCK_GAS_LIMIT * 10 + 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect((result as any).error.message).to.be.equal(
			"provided gas limit is too high (can be up to 10x the block gas limit)"
		);
	});

	step("should estimateGas with gas limit under block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${BLOCK_GAS_LIMIT.toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal("0x3043a");
	});

	step("should estimateGas with gas limit up to 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(BLOCK_GAS_LIMIT * 10).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal("0x3043a");
	});

	step("shouldn't estimateGas with gas limit up higher than 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(BLOCK_GAS_LIMIT * 20 + 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.not.exist;
		expect((result as any).error.message).to.be.equal(
			"provided gas limit is too high (can be up to 10x the block gas limit)"
		);
	});

	step("should use the gas limit multiplier fallback", async function () {
		const contract = new context.web3.eth.Contract(FORCE_GAS_CONTRACT_ABI);

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: FORCE_GAS_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		const block = await context.web3.eth.getBlock("latest");

		let receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		let contractAddress = receipt.contractAddress;

		// When not specifying gas we expect the gas limit to default to a 10x block gas limit
		// non-transactional call. The contract's method used requires close to block gas limit * 10.
		const result = await customRequest(context.web3, "eth_call", [
			{
				to: contractAddress,
				// require something close to the block gas limit * 10
				data: contract.methods.force_gas(block.gasLimit * 10 - 500_000).encodeABI(),
			},
		]);

		expect(result).to.include({
			jsonrpc: "2.0",
			result: "0x0000000000000000000000000000000000000000000000000000000000000001",
			id: 1,
		});
	});
});
