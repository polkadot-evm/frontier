import { expect } from "chai";
import fs from "fs";

import Test from "../build/contracts/Test.json"
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";
import { AbiItem } from "web3-utils";

const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
const GENESIS_ACCOUNT_PRIVATE_KEY =
    "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

const MODEXP_CONTRACT_ADDRESS = "0000000000000000000000000000000000000005";

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
		const cases = JSON.parse(data);

		for (const testCase of cases) {
			console.log("Executing test case "+ testCase.Name);

			const callResult = await context.web3.eth.call(
				{
					from: GENESIS_ACCOUNT,
					to: MODEXP_CONTRACT_ADDRESS,
					data: testCase.Input,
					value: "0x00",
					gasPrice: "0x01",
					gas: "0x10000",
				}
			);

			expect(callResult).equals("0x"+testCase.Expected);

			const estimateGasResult = await context.web3.eth.estimateGas(
				{
					from: GENESIS_ACCOUNT,
					to: MODEXP_CONTRACT_ADDRESS,
					data: testCase.Input,
				}
			);
			console.log("estimateGas: "+ estimateGasResult + " expectedGas: "+ testCase.Gas +" diff: "+ (estimateGasResult - testCase.Gas));
		}
	}).timeout(60000);

});
