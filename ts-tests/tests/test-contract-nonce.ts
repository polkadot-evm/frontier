import { expect } from "chai";

import Lock from "../build/contracts/Lock.json";
import Lockdrop from "../build/contracts/Lockdrop.json";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";
import { AbiItem } from "web3-utils";
const contract = require("@truffle/contract");
const rlp = require('rlp');
const keccak = require('keccak');
const BN = require('bn.js');

describeWithFrontier("Frontier RPC (Contract Methods)", (context) => {
	const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
	const GENESIS_ACCOUNT_PRIVATE_KEY = "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

	const TEST_CONTRACT_BYTECODE = Lockdrop.bytecode;
	const TEST_CONTRACT_ABI = Lockdrop.abi as AbiItem[];
	const FIRST_CONTRACT_ADDRESS = "0xc2bf5f29a4384b1ab0c063e1c666f02121b6084a"; // Those test are ordered. In general this should be avoided, but due to the time it takes	// to spin up a frontier node, it saves a lot of time.
	let LD;

	before("create the contract", async function () {
		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x01",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
	});

	it("get transaction by hash", async () => {
		const latestBlock = await context.web3.eth.getBlock("latest");
		expect(latestBlock.transactions.length).to.equal(1);

		const tx_hash = latestBlock.transactions[0];
		const tx = await context.web3.eth.getTransaction(tx_hash);
		expect(tx.hash).to.equal(tx_hash);
	});

	it("should return contract method result", async function () {
		const web3 = context.web3;
		const SECONDS_IN_DAY = 86400;
		const THREE_MONTHS = 0;
		const SIX_MONTHS = 1;
		const TWELVE_MONTHS = 2;

		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x01",
		});
		// await contract.methods
		// 	.multiply()
		// 	.call()
		// 	.catch((err) =>
		// 		expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
		// 	);

	    let startNonce = await web3.eth.getTransactionCount(FIRST_CONTRACT_ADDRESS);
	    console.log('Start nonce', startNonce);
	    expect(startNonce).to.equal(0, 'start nonce of deployed contract should be 0');

	    let senderBalance = new BN(await web3.eth.getBalance(GENESIS_ACCOUNT));

	    const bcontractAddr1 = getContractAddress(FIRST_CONTRACT_ADDRESS, startNonce);
	    const bcontractAddr2 = getContractAddress(FIRST_CONTRACT_ADDRESS, startNonce + 1)
	    const bcontractAddr3 = getContractAddress(FIRST_CONTRACT_ADDRESS, startNonce + 2);
	    const bcontractAddr4 = getContractAddress(FIRST_CONTRACT_ADDRESS, startNonce + 3);

	    const value = web3.utils.toWei('10', 'ether');

	    let before_nonce = await web3.eth.getTransactionCount(FIRST_CONTRACT_ADDRESS);
	    console.log('Before lock nonce', before_nonce);

		const data = contract.methods.lock(THREE_MONTHS, GENESIS_ACCOUNT, true).encodeABI();
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				to: FIRST_CONTRACT_ADDRESS,
				data: data,
				value: "0x10",
				gasPrice: "0x01",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		expect(await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).to.include({
			id: 1,
			jsonrpc: "2.0",
		});
		await createAndFinalizeBlock(context.web3);
		let receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		console.log(receipt);
			// .send({
			// 	from: GENESIS_ACCOUNT,
			// 	value: value,
			// 	gas: 1500000,
			// })
			// .catch((err) =>
			// 	expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
			// );
	    // const res = await lockdrop.lock(THREE_MONTHS, GENESIS_ACCOUNT, true, {
	    //   from: GENESIS_ACCOUNT,
	    //   value: value,
	    //   gas: 1500000,
	    //   gasPrice: 1000,
	    // });
	    let after_nonce = await web3.eth.getTransactionCount(FIRST_CONTRACT_ADDRESS);
	    console.log('After lock nonce', after_nonce);

	    let balLock1 = await web3.eth.getBalance(bcontractAddr1);
	    let balLock2 = await web3.eth.getBalance(bcontractAddr2);
	    let balLock3 = await web3.eth.getBalance(bcontractAddr3);
	    let balLock4 = await web3.eth.getBalance(bcontractAddr4);

	    expect(value.toString()).to.equal(balLock1, 'balance of first lock does not match expected');
	    expect(0).to.equal(balLock2, 'balance of future second lock does not match expected');
	    expect(0).to.equal(balLock3, 'balance of future third lock does not match expected');
	    expect(0).to.equal(balLock4, 'balance of future fourth lock does not match expected');

	    let senderBalanceAfter = new BN(await web3.eth.getBalance(GENESIS_ACCOUNT));
	    let sentBalance = senderBalance.sub(senderBalanceAfter);
	    expect(sentBalance).to.be.gt(new BN(value), 'sent balance should be greater than lock value');

	    const nonce = await web3.eth.getTransactionCount(FIRST_CONTRACT_ADDRESS);
	    console.log('Second nonce', nonce);
	    const contractAddr = getContractAddress(FIRST_CONTRACT_ADDRESS, nonce);
	    expect(nonce).to.equal(1, 'contract nonce of Lockdrop contract should be 1 after lock')

	    const bal0 = await web3.eth.getBalance(contractAddr);

	    expect(bal0).to.equal(value, 'Lock value at address should be 10 eth after lock');

	    const value2 = web3.utils.toWei('100', 'ether');

	    // await LD.lock(THREE_MONTHS, GENESIS_ACCOUNT, true, {
	    //   from: GENESIS_ACCOUNT,
	    //   value: value2,
	    //   gas: 1500000,
	    //   gasPrice: 1000000000,
	    // });

		await contract.methods
			.lock(THREE_MONTHS, GENESIS_ACCOUNT, true)
			.send({
				from: GENESIS_ACCOUNT,
				value: value2,
				gas: 1500000,
			})
			.catch((err) =>
				expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
			);

	    const new_nonce = await context.web3.eth.getTransactionCount(FIRST_CONTRACT_ADDRESS);
	    const new_contractAddr = getContractAddress(FIRST_CONTRACT_ADDRESS, new_nonce - 1);
	    const bal2 = await context.web3.eth.getBalance(new_contractAddr);

	    expect(bal2).to.equal(value2, '2nd lock value should be non zero after lock');
	    expect(new_nonce - 1).to.equal(nonce, 'nonce should increment');

	    balLock1 = await context.web3.eth.getBalance(bcontractAddr1);
	    balLock2 = await context.web3.eth.getBalance(bcontractAddr2);
	    balLock3 = await context.web3.eth.getBalance(bcontractAddr3);
	    balLock4 = await context.web3.eth.getBalance(bcontractAddr4);

	    expect(value.toString()).to.equal(balLock1, 'balance of first lock does not match expected');
	    expect(value2.toString()).to.equal(balLock2, 'balance of second lock does not match expected');
	    expect(0).to.equal(balLock3, 'balance of future third lock does not match expected');
	    expect(0).to.equal(balLock4, 'balance of future fourth lock does not match expected');
	});
});

function getContractAddress(address, nonce)  {
  const input = [address, nonce]
  const rlpEncoded = rlp.encode(input);
  const contractAddressLong = keccak('keccak256').update(rlpEncoded).digest('hex');
  const contractAddr = contractAddressLong.substring(24);
  return contractAddr;
}
