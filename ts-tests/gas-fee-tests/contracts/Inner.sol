pragma solidity ^0.5.16;

contract Demo1 {
    uint public data;
    function setData(uint _data) public {
        data = _data;
    }
}

contract Demo2 {
    function toSetData(Demo1 demo1, uint _data) public {
        demo1.setData(_data);
    }
}
