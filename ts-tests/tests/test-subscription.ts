import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier } from "./util";

let subscription;

describeWithFrontier("Frontier RPC (Subscription)", `simple-specs.json`, (context) => {
    step("should connect", async function (done) {
        await createAndFinalizeBlock(context.web3);
        // @ts-ignore
        const connected = context.web3.currentProvider.connected;
        expect(connected).to.equal(true);
        setTimeout(done,5000);
    }).timeout(10000);

    step("should subscribe", async function (done) {
        subscription = context.web3.eth.subscribe("newBlockHeaders", function(error, result){});

        let connected = false;
        let subscriptionId = "";
        await new Promise((resolve) => {
            subscription.on("connected", function (data) {
                connected = true;
                subscriptionId = data;
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
            subscription.on("data", function (d) {
                data = d;
                resolve();
            });
        });

		expect(data).to.be.not.null;
		expect(data).to.have.property("transactionsRoot");
		expect(data["transactionsRoot"]).to.be.eq("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");
        done();
    });
}, "ws");