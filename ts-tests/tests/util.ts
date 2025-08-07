import Web3 from "web3";
import { ethers } from "ethers";
import { JsonRpcResponse } from "web3-core-helpers";
import { spawn, ChildProcess } from "child_process";

import { NODE_BINARY_NAME, CHAIN_ID } from "./config";

export const PORT = 19931;
export const RPC_PORT = 19932;

export const DISPLAY_LOG = process.env.FRONTIER_LOG || false;
export const FRONTIER_LOG = process.env.FRONTIER_LOG || "info";
export const FRONTIER_BUILD = process.env.FRONTIER_BUILD || "release";
export const FRONTIER_BACKEND_TYPE = process.env.FRONTIER_BACKEND_TYPE || "key-value";

export const BINARY_PATH = `../target/${FRONTIER_BUILD}/${NODE_BINARY_NAME}`;
export const SPAWNING_TIME = 60000;

export async function customRequest(web3: Web3, method: string, params: any[]) {
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
export async function createAndFinalizeBlock(web3: Web3, finalize: boolean = true) {
	const response = await customRequest(web3, "engine_createBlock", [true, finalize, null]);
	if (!response.result) {
		throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
	}
	await new Promise<void>((resolve) => setTimeout(() => resolve(), 500));
}

// Create a block and finalize it.
// It will include all previously executed transactions since the last finalized block.
export async function createAndFinalizeBlockNowait(web3: Web3) {
	const response = await customRequest(web3, "engine_createBlock", [true, true, null]);
	if (!response.result) {
		throw new Error(`Unexpected result: ${JSON.stringify(response)}`);
	}
}

export async function startFrontierNode(
	provider?: string,
	additionalArgs: string[] = []
): Promise<{
	web3: Web3;
	binary: ChildProcess;
	ethersjs: ethers.JsonRpcProvider;
}> {
	let web3;
	if (!provider || provider == "http") {
		web3 = new Web3(`http://127.0.0.1:${RPC_PORT}`);
	} else if (provider == "ws") {
		web3 = new Web3(`ws://127.0.0.1:${RPC_PORT}`);
	}

	const ethersjs = new ethers.JsonRpcProvider(`http://127.0.0.1:${RPC_PORT}`, {
		chainId: CHAIN_ID,
		name: "frontier-dev",
	});

	const attachOnExisting = process.env.FRONTIER_ATTACH || false;
	if (attachOnExisting) {
		try {
			// Return with a fake binary object to maintain API compatibility
			return { web3, ethersjs, binary: null as any };
		} catch (_error) {
			console.log(`\x1b[33mNo existing node found, starting new one...\x1b[0m`);
		}
	}

	if (!provider || provider == "http") {
		web3 = new Web3(`http://127.0.0.1:${RPC_PORT}`);
	}

	const cmd = BINARY_PATH;
	const args = [
		`--chain=dev`,
		`--validator`, // Required by manual sealing to author the blocks
		`--execution=Native`, // Faster execution using native
		`--no-telemetry`,
		`--no-prometheus`,
		`--sealing=Manual`,
		`--no-grandpa`,
		`--force-authoring`,
		`-l${FRONTIER_LOG}`,
		`--port=${PORT}`,
		`--rpc-port=${RPC_PORT}`,
		`--frontier-backend-type=${FRONTIER_BACKEND_TYPE}`,
		`--tmp`,
		`--unsafe-force-node-key-generation`,
		...additionalArgs,
	];
	const binary = spawn(cmd, args);

	binary.on("error", (err) => {
		if ((err as any).errno == "ENOENT") {
			console.error(
				`\x1b[31mMissing Frontier binary (${BINARY_PATH}).\nPlease compile the Frontier project:\ncargo build\x1b[0m`
			);
		} else {
			console.error(err);
		}
		process.exit(1);
	});

	const binaryLogs = [];
	await new Promise<void>((resolve) => {
		const timer = setTimeout(() => {
			console.error(`\x1b[31m Failed to start Frontier Template Node.\x1b[0m`);
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
			if (chunk.toString().match(/Manual Seal Ready/)) {
				if (!provider || provider == "http") {
					// This is needed as the EVM runtime needs to warmup with a first call
					await web3.eth.getChainId();
				}

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

	if (provider == "ws") {
		web3 = new Web3(`ws://127.0.0.1:${RPC_PORT}`);
	}

	return { web3, binary, ethersjs };
}

export function describeWithFrontier(
	title: string,
	cb: (context: { web3: Web3 }) => void,
	provider?: string,
	additionalArgs: string[] = []
) {
	describe(title, () => {
		let context: {
			web3: Web3;
			ethersjs: ethers.JsonRpcProvider;
		} = { web3: null, ethersjs: null };
		let binary: ChildProcess;
		// Making sure the Frontier node has started
		before("Starting Frontier Test Node", async function () {
			this.timeout(SPAWNING_TIME);
			const init = await startFrontierNode(provider, additionalArgs);
			context.web3 = init.web3;
			context.ethersjs = init.ethersjs;
			binary = init.binary;
		});

		after(async function () {
			//console.log(`\x1b[31m Killing RPC\x1b[0m`);
			if (binary) {
				binary.kill();
			}
		});

		cb(context);
	});
}

export function describeWithFrontierFaTp(title: string, cb: (context: { web3: Web3 }) => void) {
	describeWithFrontier(title, cb, undefined, [`--pool-type=fork-aware`]);
}

export function describeWithFrontierSsTp(title: string, cb: (context: { web3: Web3 }) => void) {
	describeWithFrontier(title, cb, undefined, [`--pool-type=single-state`]);
}

export function describeWithFrontierAllPools(title: string, cb: (context: { web3: Web3 }) => void) {
	describeWithFrontierSsTp(`[SsTp] ${title}`, cb);
	describeWithFrontierFaTp(`[FaTp] ${title}`, cb);
}

export function describeWithFrontierWs(title: string, cb: (context: { web3: Web3 }) => void) {
	describeWithFrontier(title, cb, "ws");
}
