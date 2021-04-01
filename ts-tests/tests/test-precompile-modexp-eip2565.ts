import { expect } from "chai";
import fs from "fs";

import Test from "../build/contracts/Test.json"
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";
import { AbiItem } from "web3-utils";

const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
const GENESIS_ACCOUNT_PRIVATE_KEY =
    "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

function readFile(path) {
	return new Promise<string>((resolve, reject) => {
		fs.readFile(path, "utf8", (err, data) => {
			if (err) {
				reject(err);
			} else {
				resolve(data);
			}
		})
	})
};

describeWithFrontier("Frontier RPC (Modexp Precompile EIP-2565)", `simple-specs.json`, (context) => {
	const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";

	it("should pass all EIP2565 test cases", async function() {

		const data = await readFile("tests/modexp_eip2565.json");
		console.log("read file");
		// console.log("data => ", data);

		const cases = JSON.parse(data);
		// console.log("parsed json => ", cases);

		for (const testCase of cases) {
			console.log("Executing test case "+ testCase.Name);
			// console.log(testCase)

			console.log("About to sign txn...");
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					data: testCase.Input,
					value: "0x00",
					gasPrice: "0x01",
					gas: "0x"+ testCase.Gas.toString(16),
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			console.log("About to send...");

			const r = customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
			console.log("About to finalize block...");
			await createAndFinalizeBlock(context.web3);
			console.log("About to get receipt...");
			const receipt = await context.web3.eth.getTransactionReceipt(tx.messageHash);

			console.log("About to get assert...");
			expect(receipt.gasUsed).equals(testCase.Gas);
		}
	});

});
