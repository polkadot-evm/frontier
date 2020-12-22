const expect = require("chai").expect
const step = require("mocha-steps").step
const Web3 = require("web3")

const TIMEOUT = require("../truffle-config").mocha.timeout
const demo1 = require("../build/contracts/Demo1.json")
const demo2 = require("../build/contracts/Demo2.json")

const web3Clover = new Web3("your frontier http provider")
const GENESIS_ACCOUNT = "your endowed eth account"
const GENESIS_ACCOUNT_PRIVATE_KEY = "private key of your endowed eth account"

async function deployContract(web3, abi, bytecode, arguments) {
    let contract = new web3.eth.Contract(abi)
    let transaction = contract.deploy({data: bytecode, arguments: arguments})
    let gas = await transaction.estimateGas({
        from: GENESIS_ACCOUNT
    })
    let options = {
        value: "0x00",
        data: transaction.encodeABI(),
        gas : gas
    }
    let signedTransaction = await web3.eth.accounts.signTransaction(options, GENESIS_ACCOUNT_PRIVATE_KEY)
    let result = await web3.eth.sendSignedTransaction(signedTransaction.rawTransaction)
    return result
}

describe("Test contract", () => {
    let deployDemo1;
    step("Deploy contract (demo 1) should succeed", async () => {
        deployDemo1 = await deployContract(web3Clover, demo1.abi, demo1.bytecode, [])
        console.log('demo1 deployed successfully: ', deployDemo1)
    }).timeout(TIMEOUT)

    step("Deploy contract (demo 2) should succeed", async () => {
        let deployDemo2 = await deployContract(web3Clover, demo2.abi, demo2.bytecode, [])
        console.log('demo2 deployed successfully: ', deployDemo2)
        const demo2Contract = new web3Clover.eth.Contract(demo2.abi, deployDemo2.contractAddress)

        const tx_builder = demo2Contract.methods.toSetData(deployDemo1.contractAddress, 5)
        let gas = await tx_builder.estimateGas({
            from: GENESIS_ACCOUNT,
        })

        const signTransaction = {
            gas: gas,
            gasPrice: web3Clover.utils.toWei("1", "gwei"),
            data: tx_builder.encodeABI(),
            from: GENESIS_ACCOUNT,
            to: deployDemo2.contractAddress
        }

        let signedTransaction = await web3Clover.eth.accounts.signTransaction(signTransaction, GENESIS_ACCOUNT_PRIVATE_KEY)
        let receipt = await web3Clover.eth.sendSignedTransaction(signedTransaction.rawTransaction)
        console.log("contract method invoked successfully: ", receipt)
    }).timeout(TIMEOUT)
});
