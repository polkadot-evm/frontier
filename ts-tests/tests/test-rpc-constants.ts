import { expect } from "chai";

import { describeWithFrontier } from "./util";

// All test for the RPC

describeWithFrontier("Frontier RPC (Constant)", `simple-specs.json`, (context) => {
	it("should have 0 hashrate", async function () {
		expect(await context.web3.eth.getHashrate()).to.equal(0);
	});

	it("should have chainId 42", async function () {
		// The chainId is defined by the Substrate Chain Id, default to 42
		expect(await context.web3.eth.getChainId()).to.equal(42);
	});

	it("should have no account", async function () {
		expect(await context.web3.eth.getAccounts()).to.eql([]);
	});

	it("block author should be 0x0000000000000000000000000000000000000000", async function () {
		// This address `0x1234567890` is hardcoded into the runtime find_author
		// as we are running manual sealing consensus.
		expect(await context.web3.eth.getCoinbase()).to.equal("0x0000000000000000000000000000000000000000");
	});
});
