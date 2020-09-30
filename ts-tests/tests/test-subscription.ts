import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (Subscription)", `simple-specs.json`, (context) => {

    let subscription;

    const GENESIS_ACCOUNT = "0x6be02d1d3665660d22ff9624b7be0551ee1ac91b";
	const GENESIS_ACCOUNT_PRIVATE_KEY = "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

	// Solidity: contract test { function multiply(uint a) public pure returns(uint d) {return a * 7;}}
	const TEST_CONTRACT_BYTECODE =
		"0x6080604052348015600f57600080fd5b5060ae8061001e6000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063c6888fa114602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b600060078202905091905056fea265627a7a72315820f06085b229f27f9ad48b2ff3dd9714350c1698a37853a30136fa6c5a7762af7364736f6c63430005110032";
    const FIRST_CONTRACT_ADDRESS = "0xc2bf5f29a4384b1ab0c063e1c666f02121b6084a";
    
    async function sendTransaction(context) {
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
        return tx;
    }

    step("should connect", async function (done) {
        await createAndFinalizeBlock(context.web3);
        // @ts-ignore
        const connected = context.web3.currentProvider.connected;
        expect(connected).to.equal(true);
        setTimeout(done,8000);
    }).timeout(10000);

    step("should subscribe", async function (done) {
        subscription = context.web3.eth.subscribe("newBlockHeaders", function(error, result){});

        let connected = false;
        let subscriptionId = "";
        await new Promise((resolve) => {
            subscription.on("connected", function (d: any) {
                connected = true;
                subscriptionId = d;
                resolve();
            });
        });

		expect(connected).to.equal(true);
        expect(subscriptionId).to.have.lengthOf(16);
        done();
    });

    step("should get newHeads stream", async function (done) {
        await createAndFinalizeBlock(context.web3);
        let data = null;
        await new Promise((resolve) => {
            subscription.on("data", function (d: any) {
                data = d;
                resolve();
            });
        });

		expect(data).to.be.not.null;
		expect(data).to.have.property("transactionsRoot");
		expect(data["transactionsRoot"]).to.be.eq("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");
        setTimeout(done,8000);
    }).timeout(10000);

    step("should get newPendingTransactions stream", async function (done) {
        subscription = context.web3.eth.subscribe("pendingTransactions", function(error, result){});

        await new Promise((resolve) => {
            subscription.on("connected", function (d: any) {
                resolve();
            });
        });

        const tx = await sendTransaction(context);
        
        await createAndFinalizeBlock(context.web3);

        let data = null;
        await new Promise((resolve) => {
            subscription.on("data", function (d: any) {
                data = d;
                resolve();
            });
        });

        expect(data).to.be.not.null;
        expect(tx["transactionHash"]).to.be.eq(data);
        done();
    });
}, "ws");