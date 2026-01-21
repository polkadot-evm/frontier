import { assert, expect } from "chai";
import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

// MODEXP precompile address (0x05)
const MODEXP_PRECOMPILE_ADDRESS = "0x0000000000000000000000000000000000000005";

// EIP-7823: Maximum input size limit (1024 bytes)
const EIP7823_INPUT_SIZE_LIMIT = 1024;

/**
 * Encode MODEXP input according to EIP-198:
 * - 32 bytes: length of base (big-endian)
 * - 32 bytes: length of exponent (big-endian)
 * - 32 bytes: length of modulus (big-endian)
 * - base bytes
 * - exponent bytes
 * - modulus bytes
 */
function encodeModexpInput(
	baseLen: number,
	expLen: number,
	modLen: number,
	base: string,
	exp: string,
	mod: string
): string {
	const baseLenHex = baseLen.toString(16).padStart(64, "0");
	const expLenHex = expLen.toString(16).padStart(64, "0");
	const modLenHex = modLen.toString(16).padStart(64, "0");

	return "0x" + baseLenHex + expLenHex + modLenHex + base + exp + mod;
}

/**
 * Create a hex string of specified byte length filled with zeros
 */
function zeroBytes(length: number): string {
	return "00".repeat(length);
}

describeWithFrontier("Frontier RPC (MODEXP Precompile - EIP-7823)", (context) => {
	it("should perform basic modexp: 3^5 mod 7 = 5", async function () {
		// 3^5 mod 7 = 243 mod 7 = 5
		const input = encodeModexpInput(
			1, // base length
			1, // exp length
			1, // mod length
			"03", // base = 3
			"05", // exp = 5
			"07" // mod = 7
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0x100000",
		});

		// Result should be 5 (0x05), padded to modulus length (1 byte)
		assert.equal(result, "0x05");
	});

	it("should perform modexp with larger numbers: 2^10 mod 1000 = 24", async function () {
		// 2^10 mod 1000 = 1024 mod 1000 = 24
		const input = encodeModexpInput(
			1, // base length
			1, // exp length
			2, // mod length
			"02", // base = 2
			"0a", // exp = 10
			"03e8" // mod = 1000 (0x3e8)
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0x100000",
		});

		// Result should be 24 (0x18), padded to modulus length (2 bytes)
		assert.equal(result, "0x0018");
	});

	it("should return zero when modulus is 1", async function () {
		// Any number mod 1 = 0
		const input = encodeModexpInput(
			1, // base length
			1, // exp length
			1, // mod length
			"ff", // base = 255
			"ff", // exp = 255
			"01" // mod = 1
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0x100000",
		});

		assert.equal(result, "0x00");
	});

	it("should return empty output when modulus length is 0", async function () {
		const input = encodeModexpInput(
			1, // base length
			1, // exp length
			0, // mod length = 0
			"03", // base
			"05", // exp
			"" // no modulus
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0x100000",
		});

		assert.equal(result, "0x");
	});

	// EIP-7823 Tests

	it("EIP-7823: should succeed with base length at exactly 1024 bytes", async function () {
		this.timeout(30000);

		// Create input with base_length = 1024 (at the limit)
		// base = 2 (padded to 1024 bytes), exp = 3, mod = 5
		// 2^3 mod 5 = 8 mod 5 = 3
		const baseHex = zeroBytes(1023) + "02"; // 1024 bytes with value 2 at end
		const input = encodeModexpInput(
			EIP7823_INPUT_SIZE_LIMIT, // base length = 1024
			1, // exp length
			1, // mod length
			baseHex, // base (1024 bytes)
			"03", // exp = 3
			"05" // mod = 5
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0xF42400", // 16M gas
		});

		// 2^3 mod 5 = 3
		assert.equal(result, "0x03");
	});

	it("EIP-7823: should succeed with exponent length at exactly 1024 bytes", async function () {
		this.timeout(30000);

		// Create input with exp_length = 1024 (at the limit)
		// base = 2, exp = 3 (padded to 1024 bytes), mod = 5
		const expHex = zeroBytes(1023) + "03"; // 1024 bytes with value 3 at end
		const input = encodeModexpInput(
			1, // base length
			EIP7823_INPUT_SIZE_LIMIT, // exp length = 1024
			1, // mod length
			"02", // base = 2
			expHex, // exp (1024 bytes)
			"05" // mod = 5
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0xF42400", // 16M gas
		});

		// 2^3 mod 5 = 3
		assert.equal(result, "0x03");
	});

	it("EIP-7823: should succeed with modulus length at exactly 1024 bytes", async function () {
		this.timeout(30000);

		// Create input with mod_length = 1024 (at the limit)
		// base = 2, exp = 3, mod = 5 (padded to 1024 bytes)
		const modHex = zeroBytes(1023) + "05"; // 1024 bytes with value 5 at end
		const input = encodeModexpInput(
			1, // base length
			1, // exp length
			EIP7823_INPUT_SIZE_LIMIT, // mod length = 1024
			"02", // base = 2
			"03", // exp = 3
			modHex // mod (1024 bytes)
		);

		const result = await context.web3.eth.call({
			to: MODEXP_PRECOMPILE_ADDRESS,
			from: GENESIS_ACCOUNT,
			data: input,
			gas: "0xF42400", // 16M gas
		});

		// 2^3 mod 5 = 3, but result is padded to 1024 bytes
		const expected = "0x" + zeroBytes(1023) + "03";
		assert.equal(result, expected);
	});

	it("EIP-7823: should fail when base length exceeds 1024 bytes", async function () {
		// Create input with base_length = 1025 (exceeds limit)
		const baseLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");
		const expLenHex = "1".padStart(64, "0");
		const modLenHex = "1".padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;

		try {
			await context.web3.eth.call({
				to: MODEXP_PRECOMPILE_ADDRESS,
				from: GENESIS_ACCOUNT,
				data: input,
				gas: "0x100000",
			});
			assert.fail("Expected call to revert");
		} catch (error: any) {
			// The call should fail with EIP-7823 error
			expect(error.message).to.include("EIP-7823");
		}
	});

	it("EIP-7823: should fail when exponent length exceeds 1024 bytes", async function () {
		// Create input with exp_length = 1025 (exceeds limit)
		const baseLenHex = "1".padStart(64, "0");
		const expLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");
		const modLenHex = "1".padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;

		try {
			await context.web3.eth.call({
				to: MODEXP_PRECOMPILE_ADDRESS,
				from: GENESIS_ACCOUNT,
				data: input,
				gas: "0x100000",
			});
			assert.fail("Expected call to revert");
		} catch (error: any) {
			// The call should fail with EIP-7823 error
			expect(error.message).to.include("EIP-7823");
		}
	});

	it("EIP-7823: should fail when modulus length exceeds 1024 bytes", async function () {
		// Create input with mod_length = 1025 (exceeds limit)
		const baseLenHex = "1".padStart(64, "0");
		const expLenHex = "1".padStart(64, "0");
		const modLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;

		try {
			await context.web3.eth.call({
				to: MODEXP_PRECOMPILE_ADDRESS,
				from: GENESIS_ACCOUNT,
				data: input,
				gas: "0x100000",
			});
			assert.fail("Expected call to revert");
		} catch (error: any) {
			// The call should fail with EIP-7823 error
			expect(error.message).to.include("EIP-7823");
		}
	});

	it("EIP-7823: should consume all gas when base length exceeds limit (via transaction)", async function () {
		this.timeout(30000);

		// Create input with base_length = 1025 (exceeds limit)
		const baseLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");
		const expLenHex = "1".padStart(64, "0");
		const modLenHex = "1".padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;
		const gasLimit = 100000;

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: MODEXP_PRECOMPILE_ADDRESS,
				data: input,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: gasLimit,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		// Get the transaction receipt
		const receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);

		// EIP-7823: Transaction should fail (status = false)
		assert.equal(receipt.status, false, "Transaction should fail");

		// EIP-7823: All gas should be consumed
		assert.equal(
			receipt.gasUsed.toString(),
			gasLimit.toString(),
			"EIP-7823 requires all gas to be consumed when base length exceeds limit"
		);
	});

	it("EIP-7823: should consume all gas when exponent length exceeds limit (via transaction)", async function () {
		this.timeout(30000);

		// Create input with exp_length = 1025 (exceeds limit)
		const baseLenHex = "1".padStart(64, "0");
		const expLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");
		const modLenHex = "1".padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;
		const gasLimit = 100000;

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: MODEXP_PRECOMPILE_ADDRESS,
				data: input,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: gasLimit,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		// Get the transaction receipt
		const receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);

		// EIP-7823: Transaction should fail (status = false)
		assert.equal(receipt.status, false, "Transaction should fail");

		// EIP-7823: All gas should be consumed
		assert.equal(
			receipt.gasUsed.toString(),
			gasLimit.toString(),
			"EIP-7823 requires all gas to be consumed when exponent length exceeds limit"
		);
	});

	it("EIP-7823: should consume all gas when modulus length exceeds limit (via transaction)", async function () {
		this.timeout(30000);

		// Create input with mod_length = 1025 (exceeds limit)
		const baseLenHex = "1".padStart(64, "0");
		const expLenHex = "1".padStart(64, "0");
		const modLenHex = (EIP7823_INPUT_SIZE_LIMIT + 1).toString(16).padStart(64, "0");

		const input = "0x" + baseLenHex + expLenHex + modLenHex;
		const gasLimit = 100000;

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: MODEXP_PRECOMPILE_ADDRESS,
				data: input,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: gasLimit,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		// Get the transaction receipt
		const receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);

		// EIP-7823: Transaction should fail (status = false)
		assert.equal(receipt.status, false, "Transaction should fail");

		// EIP-7823: All gas should be consumed
		assert.equal(
			receipt.gasUsed.toString(),
			gasLimit.toString(),
			"EIP-7823 requires all gas to be consumed when modulus length exceeds limit"
		);
	});
});
