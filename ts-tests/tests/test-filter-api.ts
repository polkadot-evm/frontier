import { expect } from "chai";
import { step } from "mocha-steps";
import { create } from "ts-node";

import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (EthFilterApi)", `simple-specs.json`, (context) => {

	step("should create a Log filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newFilter", [{
			"fromBlock": "0x1",
			"toBlock": "0x2",
			"address": "0x8888f1f195afa192cfee860698584c030f4c9db1",
			"topics": ["0x000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b"]
		}]);
		expect(create_filter.result).to.be.eq("0x1");
	});
	
	step("should increment filter ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newFilter", [{
			"fromBlock": "0x1",
			"toBlock": "0x2",
			"address": "0x8888f1f195afa192cfee860698584c030f4c9db1",
			"topics": ["0x000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b"]
		}]);
		expect(create_filter.result).to.be.eq("0x2");
	});
	
	step("should create a Block filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newBlockFilter", []);
		expect(create_filter.result).to.be.eq("0x3");
	});
	
	step("should create a Pending Transaction filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newPendingTransactionFilter", []);
		expect(create_filter.result).to.be.eq("0x4");
    });
    
    step("should return responses for Block filter polling.", async function () {
        let block = await context.web3.eth.getBlock(0);
		let poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);
        
        expect(poll.result.length).to.be.eq(1);
        expect(poll.result[0]).to.be.eq(block.hash);

        await createAndFinalizeBlock(context.web3);
        
        block = await context.web3.eth.getBlock(1);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);
        
        expect(poll.result.length).to.be.eq(1);
        expect(poll.result[0]).to.be.eq(block.hash);

        await createAndFinalizeBlock(context.web3);
        await createAndFinalizeBlock(context.web3);

        block = await context.web3.eth.getBlock(2);
        let block_b = await context.web3.eth.getBlock(3);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);
        
        expect(poll.result.length).to.be.eq(2);
        expect(poll.result[0]).to.be.eq(block.hash);
        expect(poll.result[1]).to.be.eq(block_b.hash);
	});

});
