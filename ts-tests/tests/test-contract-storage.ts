import { expect } from "chai";

import Test from "../build/contracts/Storage.json"
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";
import { AbiItem } from "web3-utils";

describeWithFrontier("Frontier RPC (Contract)", `simple-specs.json`, (context) => {
	const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
	const GENESIS_ACCOUNT_PRIVATE_KEY = "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

	const TEST_CONTRACT_BYTECODE = Test.bytecode;
	const TEST_CONTRACT_ABI = Test.abi as AbiItem[];
	const TEST_CONTRACT_DEPLOYED_BYTECODE = Test.deployedBytecode;

	it("eth_getStorageAt", async function () {
		
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI);

		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x01",
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
	
		expect(getStorage0.result).to.be.eq(
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
	
		const tx1 = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: contractAddress,
				data: contract.methods
					.setStorage(
						"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
						"0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
					).encodeABI(),
				value: "0x00",
				gasPrice: "0x01",
				gas: "0x500000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
	
		await customRequest(context.web3, "eth_sendRawTransaction", [tx1.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		let receip1 = await context.web3.eth.getTransactionReceipt(tx1.transactionHash);
	
		let getStorage1 = await customRequest(context.web3, "eth_getStorageAt", [
			contractAddress,
			"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
			"latest",
		]);
	
		expect(getStorage1.result).to.be.eq(
			"0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
		);
	});
});