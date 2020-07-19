import { expect } from "chai";

import { describeWithFrontier, customRequest, createAndFinalizeBlock } from "./util";

describeWithFrontier("Frontier RPC (Gas)", `simple-specs.json`, (context) => {
	const GENESIS_ACCOUNT = "0x57d213d0927ccc7596044c6ba013dd05522aacba";
	// Solidity: contract test { function multiply(uint a) public pure returns(uint d) {return a * 7;}}
	const TEST_CONTRACT_BYTECODE =
		"0x6080604052348015600f57600080fd5b5060ae8061001e6000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063c6888fa114602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b600060078202905091905056fea265627a7a72315820f06085b229f27f9ad48b2ff3dd9714350c1698a37853a30136fa6c5a7762af7364736f6c63430005110032";

	it("eth_estimateGas for contract creation", async function () {
		expect(
			await context.web3.eth.estimateGas({
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
			})
		).to.equal(91019);
	});

	it("block gas limit over 5M", async function () {
		expect(
			(await context.web3.eth.getBlock("latest")).gasLimit
		).to.be.above(5000000);
	});

	// Testing the gas limit protection, hardcoded to 25M
	it("gas limit should decrease on next block if gas unused", async function () {
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
});
