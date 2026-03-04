import { expect, use as chaiUse } from "chai";
import chaiAsPromised from "chai-as-promised";
import { AbiItem } from "web3-utils";

import SelfDestructAfterCreate2 from "../build/contracts/SelfDestructAfterCreate2.json";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, FIRST_CONTRACT_ADDRESS } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier, waitForReceipt } from "./util";

chaiUse(chaiAsPromised);

describeWithFrontier("Test self-destruct contract", (context) => {
	const TEST_CONTRACT_BYTECODE = SelfDestructAfterCreate2.bytecode;
	const TEST_CONTRACT_DEPLOYED_BYTECODE = SelfDestructAfterCreate2.deployedBytecode;
	const TEST_CONTRACT_ABI = SelfDestructAfterCreate2.abi as AbiItem[];

	// Those test are ordered. In general this should be avoided, but due to the time it takes
	// to spin up a frontier node, it saves a lot of time.

	it("SELFDESTRUCT must reset contract account", async function () {
		this.timeout(60000);

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

		// Verify the contract is not yet stored
		expect(await customRequest(context.web3, "eth_getCode", [FIRST_CONTRACT_ADDRESS])).to.deep.equal({
			id: 1,
			jsonrpc: "2.0",
			result: "0x",
		});

		// Verify the contract is stored after the block is produced
		await createAndFinalizeBlock(context.web3);
		expect(await customRequest(context.web3, "eth_getCode", [FIRST_CONTRACT_ADDRESS])).to.deep.equal({
			id: 1,
			jsonrpc: "2.0",
			result: TEST_CONTRACT_DEPLOYED_BYTECODE,
		});

		let nonce = await context.web3.eth.getTransactionCount(GENESIS_ACCOUNT);
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});

		const sendTx = async (data: string, txNonce: number) => {
			const signed = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: FIRST_CONTRACT_ADDRESS,
					data,
					value: "0x00",
					gasPrice: "0x3B9ACA00",
					gas: "0x100000",
					nonce: txNonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [signed.rawTransaction]))
				.result as string;
			return txHash;
		};

		const tx1Hash = await sendTx(contract.methods.step1().encodeABI(), nonce++);
		const tx2Hash = await sendTx(contract.methods.step2().encodeABI(), nonce++);
		const tx3Hash = await sendTx(contract.methods.cannotRecreateInTheSameCall().encodeABI(), nonce++);

		await createAndFinalizeBlock(context.web3);

		const tx1Receipt = await waitForReceipt(context.web3, tx1Hash);
		const tx2Receipt = await waitForReceipt(context.web3, tx2Hash);
		const tx3Receipt = await waitForReceipt(context.web3, tx3Hash);

		for (const receipt of [tx1Receipt, tx2Receipt, tx3Receipt]) {
			expect(receipt.status).to.be.true;
		}

		const deployedAddress = await contract.methods.deployed1().call();

		// Verify the contract no longer exists
		expect(await customRequest(context.web3, "eth_getCode", [deployedAddress])).to.deep.equal({
			id: 1,
			jsonrpc: "2.0",
			result: "0x",
		});
	});
});
