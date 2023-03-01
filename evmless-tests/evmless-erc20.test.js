const { ethers } = require("ethers");
const { solidity } = require("ethereum-waffle");
const chai = require('chai');
const expect = chai.expect;

chai.use(solidity);

describe('tests', function () {
  const initialSupply = new ethers.BigNumber.from("1337");
  const name = "Test";
  const symbol = "TEST";
  const decimals = 12;
  const evmlessErc20Address = "0x0000000000000000000000000000000000000539";
  const evmlessErc20Abi = [
    "function name() public view returns (string)",
    "function symbol() public view returns (string)",
    "function decimals() public view returns (uint8)",
    "function totalSupply() public view returns (uint256)",
    "function balanceOf(address _owner) public view returns (uint256 balance)",
    "function transfer(address _to, uint256 _value) public returns (bool success)",
    "function transferFrom(address _from, address _to, uint256 _value) public returns (bool success)",
    "function approve(address _spender, uint256 _value) public returns (bool success)",
    "function allowance(address _owner, address _spender) public view returns (uint256 remaining)",
  ];

  before(async function () {
    this.Provider = new ethers.providers.JsonRpcProvider('http://localhost::9933');
    this.evmlessErc20Contract = new ethers.Contract(evmlessErc20Address, evmlessErc20Abi, this.Provider);
  });

  it('correct initial supply', async function () {
    expect(await this.evmlessErc20Contract.totalSupply()).to.equal(initialSupply);
  });

  it('correct name', async function () {
    expect(await this.evmlessErc20Contract.name()).to.equal(name);
  });

  it('correct symbol', async function () {
    expect(await this.evmlessErc20Contract.symbol()).to.equal(symbol);
  });

  it('correct decimals', async function () {
    expect(await this.evmlessErc20Contract.decimals()).to.equal(decimals);
  });
});
