import { expect } from "chai";

import Test from "../build/contracts/Test.json"
import { describeWithFrontier, createAndFinalizeBlock,customRequest } from "./util";
import { AbiItem } from "web3-utils";


// (!) The implementation must match the one in the rpc handler.
// If the variation in the estimate is less than 10%,
// then the estimate is considered sufficiently accurate.
const ESTIMATION_VARIANCE = 10;
function binary_search(one_off_estimation) {
	let highest = 4_294_967_295; // max(u32)
	let lowest = 21000;
	let mid = Math.min(one_off_estimation * 3, (highest + lowest) / 2);
	let previous_highest = highest;
	while(true) {
		if(mid >= one_off_estimation) {
			highest = mid;
			if((previous_highest - highest) * ESTIMATION_VARIANCE / previous_highest < 1){
				break;
			}
			previous_highest = highest;
		} else {
			lowest = mid;
		}
		mid = Math.floor((highest + lowest) / 2);
	}
	return highest;
}

function estimation_variance(binary_search_estimation, one_off_estimation) {
	return (binary_search_estimation - one_off_estimation) * ESTIMATION_VARIANCE / binary_search_estimation;
}

describeWithFrontier("Frontier RPC (Gas)", (context) => {
	const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";

	const TEST_CONTRACT_BYTECODE = Test.bytecode;
	const TEST_CONTRACT_ABI = Test.abi as AbiItem[];
	const FIRST_CONTRACT_ADDRESS = "0xc2bf5f29a4384b1ab0c063e1c666f02121b6084a"; // Those test are ordered. In general this should be avoided, but due to the time it takes	// to spin up a frontier node, it saves a lot of time.

	it("eth_estimateGas for contract creation", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let one_off_estimation = 196657;
		let binary_search_estimation = binary_search(one_off_estimation);
		// Sanity check expect a variance of 10%.
		expect(
			estimation_variance(binary_search_estimation, one_off_estimation)
		).to.be.lessThan(1);
		expect(
			await context.web3.eth.estimateGas({
				from: GENESIS_ACCOUNT,
				data: Test.bytecode,
			})
		).to.equal(binary_search_estimation);
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
		let one_off_estimation = 21204;
		let binary_search_estimation = binary_search(one_off_estimation);
		// Sanity check expect a variance of 10%.
		expect(
			estimation_variance(binary_search_estimation, one_off_estimation)
		).to.be.lessThan(1);
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});

		expect(await contract.methods.multiply(3).estimateGas()).to.equal(binary_search_estimation);
	});

	it("eth_estimateGas without gas_limit should pass", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		let one_off_estimation = 21204;
		let binary_search_estimation = binary_search(one_off_estimation);
		// Sanity check expect a variance of 10%.
		expect(
			estimation_variance(binary_search_estimation, one_off_estimation)
		).to.be.lessThan(1);
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT
		});

		expect(await contract.methods.multiply(3).estimateGas()).to.equal(binary_search_estimation);
	});

	it("eth_estimateGas should handle AccessList alias", async function () {
		// The value returned as an estimation by the evm with estimate mode ON.
		// 4300 == 1900 for one key and 2400 for one storage.
		let one_off_estimation = 196657 + 4300;
		let binary_search_estimation = binary_search(one_off_estimation);
		// Sanity check expect a variance of 10%.
		expect(
			estimation_variance(binary_search_estimation, one_off_estimation)
		).to.be.lessThan(1);
		let result = (await customRequest(context.web3, "eth_estimateGas", [{
			from: GENESIS_ACCOUNT,
			data: Test.bytecode,
			accessList: [{
				address: "0x0000000000000000000000000000000000000000",
				storageKeys: ["0x0000000000000000000000000000000000000000000000000000000000000000"]
			}]
		}])).result;
		expect(result).to.equal(context.web3.utils.numberToHex(binary_search_estimation));
	});

});
