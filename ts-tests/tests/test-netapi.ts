import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Net)", (context) => {
	step("should return `net_version` 42", async function () {
		expect(await context.web3.eth.net.getId()).to.equal(42);
	});
	step("should return `peer_count` in hex directly using the provider", async function () {
		expect((await customRequest(context.web3, "net_peerCount", [])).result).to.be.eq("0x0");
	});
	step("should format `peer_count` as decimal using `web3.net`", async function () {
		expect(await context.web3.eth.net.getPeerCount()).to.equal(0);
	});
});
