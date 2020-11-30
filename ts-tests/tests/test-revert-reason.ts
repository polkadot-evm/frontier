import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";
import { AbiItem } from "web3-utils";

describeWithFrontier("Frontier RPC (Revert Reason)", `simple-specs.json`, (context) => {

	let contractAddress;

	const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
	const GENESIS_ACCOUNT_PRIVATE_KEY = "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";
	
	// contract ExplicitRevertReason {
	// 	function max10(uint256 a) public returns (uint256) {
	// 		if (a > 10)
	// 			revert("Value must not be greater than 10.");
	// 		return a;
	// 	}
	// }
	const REVERT_W_MESSAGE_BYTECODE = "0x608060405234801561001057600080fd5b50610127806100206000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c80638361ff9c14602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b6000600a82111560c7576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825260228152602001806100d06022913960400191505060405180910390fd5b81905091905056fe56616c7565206d757374206e6f742062652067726561746572207468616e2031302ea2646970667358221220e63c9905b696e005347b92b4a24ac548a70b1fa80b9d8d2c0499b795503a1b4a64736f6c634300060c0033";

	const TEST_CONTRACT_ABI = {
		constant: true,
		inputs: [{ internalType: "uint256", name: "a", type: "uint256" }],
		name: "max10",
		outputs: [{ internalType: "uint256", name: "b", type: "uint256" }],
		payable: false,
		stateMutability: "pure",
		type: "function",
	} as AbiItem;
	
	before("create the contract", async function () {
		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: REVERT_W_MESSAGE_BYTECODE,
				value: "0x00",
				gasPrice: "0x01",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		const r = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
		const receipt = await context.web3.eth.getTransactionReceipt(r.result);
		contractAddress = receipt.contractAddress;
	});

	it("should fail with revert reason", async function () {
		const contract = new context.web3.eth.Contract([TEST_CONTRACT_ABI], contractAddress, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x01",
		});
		try {
			await contract.methods.max10(30).call();
		} catch (error) {
			expect(error.message).to.be.eq(
				"Returned error: VM Exception while processing transaction: revert Value must not be greater than 10."
			);
		}
	});

});