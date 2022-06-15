import { expect } from "chai";
import { AbiItem } from "web3-utils";

import Test from "../build/contracts/Storage.json";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (Contract)", (context) => {
	const TEST_CONTRACT_BYTECODE = Test.bytecode;
	const TEST_CONTRACT_ABI = Test.abi as AbiItem[];

	it("eth_getStorageAt", async function () {
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI);

		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		expect(await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).to.include({
			id: 1,
			jsonrpc: "2.0",
		});

		await createAndFinalizeBlock(context.web3);
		let receipt0 = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		let contractAddress = receipt0.contractAddress;

		let getStorage0 = await customRequest(context.web3, "eth_getStorageAt", [
			contractAddress,
			"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
			"latest",
		]);

		expect(getStorage0.result).to.be.eq("0x0000000000000000000000000000000000000000000000000000000000000000");

		const tx1 = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: contractAddress,
				data: contract.methods
					.setStorage(
						"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
						"0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
					)
					.encodeABI(),
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x500000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx1.rawTransaction]);

		let getStoragePending = await customRequest(context.web3, "eth_getStorageAt", [
			contractAddress,
			"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
			"pending",
		]);

		const expectedStorage = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

		expect(getStoragePending.result).to.be.eq(expectedStorage);

		await createAndFinalizeBlock(context.web3);

		let getStorage1 = await customRequest(context.web3, "eth_getStorageAt", [
			contractAddress,
			"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
			"latest",
		]);

		expect(getStorage1.result).to.be.eq(expectedStorage);
	});
});
