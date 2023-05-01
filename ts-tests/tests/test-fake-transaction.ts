import { ApiPromise, WsProvider, Keyring } from "@polkadot/api";
import { u8aToHex } from "@polkadot/util";
import { xxhashAsU8a } from "@polkadot/util-crypto";
import { expect } from "chai";
import { step } from "mocha-steps";
import { ALITH_SECRET_KEY, createAndFinalizeBlock, describeWithFrontier, describeWithFrontierWs } from "./util";

describeWithFrontierWs("Frontier RPC (Fake transaction)", (context) => {
	step("should create ethereum transaction if log is injected in CurrentLogs storage", async function () {
		const api = context.polkadotApi;
		await api.isReady;

		const keyring = new Keyring({ type: "ethereum" });
		const alith = keyring.addFromUri(ALITH_SECRET_KEY);

		let module = xxhashAsU8a(new TextEncoder().encode("EVM"), 128);
		let storage = xxhashAsU8a(new TextEncoder().encode("CurrentLogs"), 128);
		let key = new Uint8Array(module.length + storage.length);
		key.set(module, 0);
		key.set(storage, module.length);

		const value = api.createType("Vec<EthereumLog>", []);
		const log = api.createType("EthereumLog", {
			address: api.createType("H160", "0x703b0c16133582Ed347eCDe0b4b8480004766Fac"),
			topics: api.createType("Vec<H256>", ["0x1111111111111111111111111111111111111111111111111111111111111111"]),
			data: api.createType("Bytes", [1, 2, 3]),
		});
		value.push(log);

		const transaction = api.tx.sudo.sudo(api.tx.system.setStorage([[u8aToHex(key), u8aToHex(value.toU8a())]]));
		await transaction.signAndSend(alith);

		await createAndFinalizeBlock(context.web3);

		{
			const lastBlockNumber = await context.web3.eth.getBlockNumber();
			const lastBlock = await context.web3.eth.getBlock(lastBlockNumber, true);
			// block contains fake transaction
			expect(lastBlock.transactions.some((tx) => tx.gas == 0 && tx.gasPrice == "0" && tx.input == "0x00000000"));
		}

		await createAndFinalizeBlock(context.web3);

		{
			const lastBlockNumber = await context.web3.eth.getBlockNumber();
			const lastBlock = await context.web3.eth.getBlock(lastBlockNumber, true);
			// block doesn't contain fake transaction
			expect(!lastBlock.transactions.some((tx) => tx.gas == 0 && tx.gasPrice == "0" && tx.input == "0x00000000"));
		}
	});
});
