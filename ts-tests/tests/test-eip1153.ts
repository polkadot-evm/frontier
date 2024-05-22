import { expect, use as chaiUse } from "chai";
import chaiAsPromised from "chai-as-promised";
import { AbiItem } from "web3-utils";

import ReentrancyProtected from "../build/contracts/ReentrancyProtected.json";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

chaiUse(chaiAsPromised);

describeWithFrontier("Frontier RPC (EIP-1153)", (context) => {
	const TEST_CONTRACT_BYTECODE = ReentrancyProtected.bytecode;
	const TEST_CONTRACT_ABI = ReentrancyProtected.abi as AbiItem[];
	let contract_address: string = null;

	// Those test are ordered. In general this should be avoided, but due to the time it takes
	// to spin up a frontier node, it saves a lot of time.

	before("create the contract", async function () {
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
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		const receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		contract_address = receipt.contractAddress;
	});

	it("should detect reentrant call and revert", async function () {
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, contract_address, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00"
		});

		try {
			await contract.methods.test().call();
		} catch (error) {
			return expect(error.message).to.be.eq(
				"Returned error: VM Exception while processing transaction: revert Reentrant call detected."
			);
		}

		expect.fail("Expected the contract call to fail");
	});
});
