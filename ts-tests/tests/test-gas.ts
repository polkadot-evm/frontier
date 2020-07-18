import { expect } from "chai";

import { describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (Gas)", `simple-specs.json`, context => {
	const GENESIS_ACCOUNT = "0x57d213d0927ccc7596044c6ba013dd05522aacba";
	// Solidity: contract test { function multiply(uint a) public pure returns(uint d) {return a * 7;}}
	const TEST_CONTRACT_BYTECODE =
		"0x6080604052348015600f57600080fd5b5060ae8061001e6000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063c6888fa114602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b600060078202905091905056fea265627a7a72315820f06085b229f27f9ad48b2ff3dd9714350c1698a37853a30136fa6c5a7762af7364736f6c63430005110032";

	it("eth_estimateGas for contract creation", async function () {
		expect(
			await context.web3.eth.estimateGas({
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE
			})
		).to.equal(91019);
	});
});
