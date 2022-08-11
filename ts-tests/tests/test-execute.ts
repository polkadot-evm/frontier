import { assert, expect } from "chai";
import { step } from "mocha-steps";
import { BLOCK_GAS_LIMIT, GENESIS_ACCOUNT } from "./config";

import { describeWithFrontier, customRequest } from "./util";

import Test from "../build/contracts/Test.json";

// EXTRINSIC_GAS_LIMIT = [BLOCK_GAS_LIMIT - BLOCK_GAS_LIMIT * (NORMAL_DISPATCH_RATIO - AVERAGE_ON_INITIALIZE_RATIO) - EXTRINSIC_BASE_Weight] / WEIGHT_PER_GAS = (1_000_000_000_000 * 2 * (0.75-0.1) - 125_000_000) / 20000
const EXTRINSIC_GAS_LIMIT = 64995685;
const TEST_CONTRACT_BYTECODE = Test.bytecode;
const TEST_CONTRACT_DEPLOYED_BYTECODE = Test.deployedBytecode;

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
});
