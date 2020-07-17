import { expect } from "chai";
import Web3 from "web3";
import { JsonRpcResponse } from "web3-core-helpers";

import { spawn, ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as rimraf from "rimraf";

const RPC_PORT = 19933;
const SPECS_PATH = `./frontier-test-specs`;
const BINARY_PATH = `../target/debug/frontier-test-node`;

const DISPLAY_LOG = process.env.FRONTIER_LOG || false;
const FRONTIER_LOG = process.env.FRONTIER_LOG || "info";

async function custom_request(web3: Web3, method: string, params: any[]) {
  return new Promise<JsonRpcResponse>((resolve, reject) => {
    (web3.currentProvider as any).send(
      {
        jsonrpc: "2.0",
        id: 1,
        method,
        params,
      },
      (error: Error | null, result?: JsonRpcResponse) => {
        if (error) {
          reject(
            `Failed to send custom request (${method} (${params.join(",")})): ${
              error.message || error.toString()
            }`
          );
        }
        resolve(result);
      }
    );
  });
}

// Create a block and finalize it.
// It will include all previously executed transactions since the last finalized block.
async function create_and_finalized_block(web3: Web3) {
  const response = await custom_request(web3, "engine_createBlock", [
    true,
    true,
    null,
  ]);
  if (!response.result) {
    throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
  }
}

const SPAWNING_TIME = 30000;

async function startFrontierNode(
  specFilename: string
): Promise<{ web3: Web3; binary: ChildProcess }> {
  const web3 = new Web3(`http://localhost:${RPC_PORT}`);

  const cmd = `../target/debug/frontier-test-node`;
  const args = [
    `--chain=${SPECS_PATH}/${specFilename}`,
    `--validator`, // Required by manual sealing to author the blocks
    `--execution=Native`, // Faster execution using native
    `--no-telemetry`,
    `--no-prometheus`,
    `--no-grandpa`,
    `--force-authoring`,
    `-l${FRONTIER_LOG}`,
    `--rpc-port=${RPC_PORT}`,
    `--ws-port=19944`, // not used
    `--tmp`,
  ];
  const binary = spawn(cmd, args);
  binary.on("error", (err) => {
    if ((err as any).errno == "ENOENT") {
      console.error(
        `\x1b[31mMissing Frontier binary (${BINARY_PATH}).\nPlease compile the Frontier project:\ncargo build --bin frontier-test-node\x1b[0m`
      );
    } else {
      console.error(err);
    }
    process.exit(1);
  });

  const binaryLogs = [];
  await new Promise((resolve) => {
    const timer = setTimeout(() => {
      console.error(`\x1b[31m Failed to start Frontier Test Node.\x1b[0m`);
      console.error(`Command: ${cmd} ${args.join(" ")}`);
      console.error(`Logs:`);
      console.error(binaryLogs.map((chunk) => chunk.toString()).join("\n"));
      process.exit(1);
    }, SPAWNING_TIME - 2000);

    const onData = async (chunk) => {
      if (DISPLAY_LOG) {
        console.log(chunk.toString());
      }
      binaryLogs.push(chunk);
      if (chunk.toString().match(/Test Node Ready/)) {
        // This is needed as the EVM runtime needs to warmup with a first call
        await web3.eth.getChainId();

        clearTimeout(timer);
        if (!DISPLAY_LOG) {
          binary.stderr.off("data", onData);
          binary.stdout.off("data", onData);
        }
        // console.log(`\x1b[31m Starting RPC\x1b[0m`);
        resolve();
      }
    };
    binary.stderr.on("data", onData);
    binary.stdout.on("data", onData);
  });

  return { web3, binary };
}
// All test for the RPC
describe("Frontier RPC ", () => {
  let binary: ChildProcess;
  let web3: Web3;

  const SPEC_FILENAME = `simple-specs.json`;
  const GENESIS_ACCOUNT = "0x57d213d0927ccc7596044c6ba013dd05522aacba";
  const GENESIS_ACCOUNT_PRIVATE_KEY =
    "0x99B3C12287537E38C90A9219D4CB074A89A16E9CDB20BF85728EBD97C343E342";

  // Solidity: contract test { function multiply(uint a) public pure returns(uint d) {return a * 7;}}
  const TEST_CONTRACT_BYTECODE =
    "0x6080604052348015600f57600080fd5b5060ae8061001e6000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c8063c6888fa114602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b600060078202905091905056fea265627a7a72315820f06085b229f27f9ad48b2ff3dd9714350c1698a37853a30136fa6c5a7762af7364736f6c63430005110032";

  // Making sure the Frontier node has started
  before("Starting Frontier Test Node", async function () {
    this.timeout(SPAWNING_TIME);
    const init = await startFrontierNode(`simple-specs.json`);
    web3 = init.web3;
    binary = init.binary;
  });

  after(async function () {
    //console.log(`\x1b[31m Killing RPC\x1b[0m`);
    binary.kill();
  });

  it("should have 0 hashrate", async function () {
    expect(await web3.eth.getHashrate()).to.equal(0);
  });

  it("should have chainId 42", async function () {
    // The chainId is defined by the Substrate Chain Id, default to 42
    expect(await web3.eth.getChainId()).to.equal(42);
  });

  it("should have no account", async function () {
    expect(await web3.eth.getAccounts()).to.eql([]);
  });

  it("genesis block number should be at 0", async function () {
    expect(await web3.eth.getBlockNumber()).to.equal(0);
  });

  it("genesis block should be null", async function () {
    expect(await web3.eth.getBlock(0)).to.be.null;
  });

  it("block author should be 0x0000000000000000000000000000001234567890", async function () {
    // This address `0x1234567890` is hardcoded into the runtime find_author
    // as we are running manual sealing consensus.
    expect(await web3.eth.getCoinbase()).to.equal(
      "0x0000000000000000000000000000001234567890"
    );
  });

  it("should be at block 1 after block production", async function () {
    this.timeout(15000);
    await create_and_finalized_block(web3);
    expect(await web3.eth.getBlockNumber()).to.equal(1);
  });

  it.skip("eth_call should be executed", async function () {
    expect(
      await web3.eth.call({
        from: GENESIS_ACCOUNT,
        to: "contract_address",
        data: "call_bytecode",
      })
    ).to.equal(1);
  });

  it("contract creation should return transaction hash", async function () {
    this.timeout(15000);
    const tx = await web3.eth.accounts.signTransaction(
      {
        from: GENESIS_ACCOUNT,
        data: TEST_CONTRACT_BYTECODE,
        value: "0x00",
        gasPrice: "0x00",
        gas: "0x100000",
      },
      GENESIS_ACCOUNT_PRIVATE_KEY
    );

    expect(
      await custom_request(web3, "eth_sendRawTransaction", [tx.rawTransaction])
    ).to.deep.equal({
      id: 1,
      jsonrpc: "2.0",
      result:
        "0xc8009207908c5caf1bae415f02562d92a290dcbe4f2fbf331bda3b7548ae6a6f",
    });

    // Verify the contract is not yet stored
    expect(
      await custom_request(web3, "eth_getCode", [
        "0xc2bf5f29a4384b1ab0c063e1c666f02121b6084a",
      ])
    ).to.deep.equal({
      id: 1,
      jsonrpc: "2.0",
      result: "0x",
    });

    // Verify the contract is stored after the block is produced
    await create_and_finalized_block(web3);
    expect(
      await custom_request(web3, "eth_getCode", [
        "0xc2bf5f29a4384b1ab0c063e1c666f02121b6084a",
      ])
    ).to.deep.equal({
      id: 1,
      jsonrpc: "2.0",
      result:
        "0x6080604052348015600f57600080fd5b506004361060285760003560e01c8063c6888fa114602d575b600080fd5b605660048036036020811015604157600080fd5b8101908080359060200190929190505050606c565b6040518082815260200191505060405180910390f35b600060078202905091905056fea265627a7a72315820f06085b229f27f9ad48b2ff3dd9714350c1698a37853a30136fa6c5a7762af7364736f6c63430005110032",
    });
  });

  it("eth_estimateGas for contract creation", async function () {
    expect(
      await web3.eth.estimateGas({
        from: GENESIS_ACCOUNT,
        data: TEST_CONTRACT_BYTECODE,
      })
    ).to.equal(91019);
  });

  // List of deprecated methods
  [
    { method: "eth_getCompilers", params: [] },
    { method: "eth_compileLLL", params: ["(returnlll (suicide (caller)))"] },
    {
      method: "eth_compileSolidity",
      params: [
        "contract test { function multiply(uint a) returns(uint d) {return a * 7;}}",
      ],
    },
    {
      method: "eth_compileSerpent",
      params: ["/* some serpent */"],
    },
  ].forEach(({ method, params }) => {
    it(`${method} should be deprecated`, async function () {
      expect(await custom_request(web3, method, params)).to.deep.equal({
        id: 1,
        jsonrpc: "2.0",
        error: { message: `Method ${method} not supported.`, code: -32600 },
      });
    });
  });
});
