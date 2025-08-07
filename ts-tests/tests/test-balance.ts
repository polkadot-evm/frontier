import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, GENESIS_ACCOUNT_BALANCE, EXISTENTIAL_DEPOSIT } from "./config";
import { createAndFinalizeBlock, describeWithTokfin, customRequest } from "./util";

describeWithTokfin("Tokfin RPC (Balance)", (context) => {
	const TEST_ACCOUNT = "0xdd33Af49c851553841E94066B54Fd28612522901";
	const TEST_ACCOUNT_PRIVATE_KEY = "0x4ca933bffe83185dda76e7913fc96e5c97cdb7ca1fbfcc085d6376e6f564ef71";
	const TRANFER_VALUE = "0x200"; // 512, must be higher than ExistentialDeposit
	const GAS_PRICE = "0x3B9ACA00"; // 1000000000
	var nonce = 0;

	step("genesis balance is setup correctly", async function () {
		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT)).to.equal(GENESIS_ACCOUNT_BALANCE);
	});

	step("balance to be updated after transfer", async function () {
		await createAndFinalizeBlock(context.web3);
		this.timeout(15000);

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: TEST_ACCOUNT,
				value: TRANFER_VALUE,
				gasPrice: GAS_PRICE,
				gas: "0x100000",
				nonce: nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);

		// GENESIS_ACCOUNT_BALANCE - (21000 * gasPrice) - value;
		const expectedGenesisBalance = (
			BigInt(GENESIS_ACCOUNT_BALANCE) -
			BigInt(21000) * BigInt(GAS_PRICE) -
			BigInt(TRANFER_VALUE)
		).toString();
		const expectedTestBalance = (Number(TRANFER_VALUE) - EXISTENTIAL_DEPOSIT).toString();
		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT, "pending")).to.equal(expectedGenesisBalance);
		expect(await context.web3.eth.getBalance(TEST_ACCOUNT, "pending")).to.equal(expectedTestBalance);

		await createAndFinalizeBlock(context.web3);

		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT)).to.equal(expectedGenesisBalance);
		expect(await context.web3.eth.getBalance(TEST_ACCOUNT)).to.equal(expectedTestBalance);
	});

	step("gas price too low", async function () {
		nonce += 1;

		let gas_price = await context.web3.eth.getGasPrice();
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: TEST_ACCOUNT,
				value: TRANFER_VALUE,
				gasPrice: Number(gas_price) - 1,
				gas: "0x100000",
				nonce: nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		let result = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		expect(result.error.message).to.be.equal("gas price less than block base fee");
	});

	step("balance insufficient", async function () {
		nonce += 1;
		let test_account_balance = await context.web3.eth.getBalance(TEST_ACCOUNT);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: TEST_ACCOUNT,
				to: GENESIS_ACCOUNT,
				value: test_account_balance + 1,
				gasPrice: GAS_PRICE,
				gas: "0x100000",
				nonce: nonce,
			},
			TEST_ACCOUNT_PRIVATE_KEY
		);
		let result = await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		expect(result.error.message).to.be.equal("insufficient funds for gas * price + value");
	});
});
