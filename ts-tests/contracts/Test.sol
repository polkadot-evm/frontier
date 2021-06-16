pragma solidity 0.8.2;

contract Test {
    function multiply(uint a) public pure returns(uint d) {
        return a * 7;
    }
    function currentBlock() public view  returns(uint) {
        return block.number;
    }
}
