import { expect } from "chai";
import { step } from "mocha-steps";
import { create } from "ts-node";

import { describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (EthFilterApi)", `simple-specs.json`, (context) => {

	step("should create a Log filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newFilter", [{
            "fromBlock": "0x1",
            "toBlock": "0x2",
            "address": "0x8888f1f195afa192cfee860698584c030f4c9db1",
            "topics": ["0x000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b"]
          }]
        );
        expect(create_filter.result).to.be.eq("0x1");
    });
    
    step("should increment filter ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newFilter", [{
            "fromBlock": "0x1",
            "toBlock": "0x2",
            "address": "0x8888f1f195afa192cfee860698584c030f4c9db1",
            "topics": ["0x000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b"]
          }]
        );
        expect(create_filter.result).to.be.eq("0x2");
    });
    
    step("should create a Block filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newBlockFilter", []
        );
        expect(create_filter.result).to.be.eq("0x3");
    });
    
    step("should create a Pending Transaction filter and return the ID", async function () {
		let create_filter = await customRequest(context.web3, "eth_newPendingTransactionFilter", []
        );
        expect(create_filter.result).to.be.eq("0x4");
    });
});
