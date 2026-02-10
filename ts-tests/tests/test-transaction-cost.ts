import { expect } from "chai";
import { step } from "mocha-steps";

import { describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Transaction cost)", (context) => {
	step("should take transaction cost into account and not submit it to the pool", async function () {
		// Protected (EIP-155) legacy tx with gas limit 0, signed offline.
		const tx = await customRequest(context.web3, "eth_sendRawTransaction", [
			"0xf86180843b9aca00809412cb274aad8251c875c0bf6872b67d9983e53fdd0180" +
				"77a05e311790fcf3eef1ed6d9ee6ffeea8c44a677b52168240b5713dacb0b85cf203" +
				"a05e55ddeb10d2ad84fdf6901ba0047d5c311de899552592e43f1d11b4947527c6",
		]);
		let msg = "intrinsic gas too low";
		expect(tx.error).to.include({
			message: msg,
		});
	});
});
