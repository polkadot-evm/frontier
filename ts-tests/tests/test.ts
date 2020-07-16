import { expect } from "chai";
import Web3 from "web3";
import { JsonRpcResponse } from "web3-core-helpers";

import { spawn, ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as rimraf from "rimraf";

const RPC_PORT = 19933;
const BASE_PATH = `./frontier-test-tmp`;
const SPECS_PATH = `./frontier-test-specs`;
const BINARY_PATH = `../target/debug/frontier-test-node`;

// Create a block and finalize it.
// It will include all previously executed transactions since the last finalized block.
async function create_and_finalized_block(web3: Web3) {
  return new Promise((resolve, reject) => {
    (web3.currentProvider as any).send(
      {
        jsonrpc: "2.0",
        id: 1,
        method: "engine_createBlock",
        params: [true, true, null],
      },
      (error: Error | null, result?: JsonRpcResponse) => {
        if (error) {
          reject(
            `Failed to send finalize block: ${
              error.message || error.toString()
            }`
          );
        }
        if (result?.result) {
          resolve(result.result);
          return;
        }
        reject(`Unexpected result: ${JSON.stringify(result)}`);
      }
    );
  });
}

const SPAWNING_TIME = 20000;

async function startFrontierNode(specFilename: string): Promise<{web3: Web3, binary: ChildProcess}> {

  if (fs.existsSync(BASE_PATH)) {
    // console.debug(`⚠  Deleting ${BASE_PATH} ⚠`);
    rimraf.sync(BASE_PATH);
  }

  const web3 = new Web3(`http://localhost:${RPC_PORT}`);

  const cmd = `../target/debug/frontier-test-node`;
  const args = [
    `--chain=${SPECS_PATH}/${specFilename}`,
    `-ldebug`,
    `--rpc-port=${RPC_PORT}`,
    `--base-path=${BASE_PATH}`
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
      console.error(`Command: ${cmd} ${args.join(' ')}`);
      console.error(`Logs:`);
      console.error(binaryLogs.map(chunk => chunk.toString()).join("\n"));
      process.exit(1);
    }, SPAWNING_TIME - 2000);

    const onData = (chunk) => {
      binaryLogs.push(chunk);
      if (chunk.toString().match(/Prometheus server started/)) {
        resolve();
        clearTimeout(timer);
        binary.stderr.off("data", onData);
      }
    };
    binary.stderr.on("data", onData);
  });

  return {web3, binary};
}
// All test for the RPC
describe("Frontier RPC ", () => {

  let binary: ChildProcess;
  let web3: Web3;

  // Making sure the Frontier node has started
  before("Starting Frontier Test Node", async function () {
    this.timeout(SPAWNING_TIME);
    const init = await startFrontierNode(`simple-specs.json`);
    web3 = init.web3;
    binary = init.binary;
  });

  after(async function () {
    binary.kill();
  });

  it("should have 0 hashrate", async () => {
    console.log("hashrate");
    expect(await web3.eth.getHashrate()).to.equal(0);
  });
});
