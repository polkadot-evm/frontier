// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @title DelegateTest
 * @dev Simple contract for EIP-7702 delegation testing
 */
contract DelegateTest {
    uint256 public constant MAGIC_NUMBER = 42;
    
    // Events
    event DelegateCall(address indexed caller, uint256 value);
    event StorageWrite(bytes32 indexed key, bytes32 value);
    
    // Storage slot for testing
    mapping(bytes32 => bytes32) public testStorage;
    
    /**
     * @dev Returns the magic number
     */
    function getMagicNumber() external pure returns (uint256) {
        return MAGIC_NUMBER;
    }
    
    /**
     * @dev Simple function that returns the caller's address
     */
    function getCaller() external view returns (address) {
        return msg.sender;
    }
    
    /**
     * @dev Function that emits an event
     */
    function emitEvent(uint256 value) external {
        emit DelegateCall(msg.sender, value);
    }
    
    /**
     * @dev Function that writes to storage
     */
    function writeStorage(bytes32 key, bytes32 value) external {
        testStorage[key] = value;
        emit StorageWrite(key, value);
    }
    
    /**
     * @dev Function that reads from storage
     */
    function readStorage(bytes32 key) external view returns (bytes32) {
        return testStorage[key];
    }
    
    /**
     * @dev Function that returns the current balance of this contract
     */
    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
    
    /**
     * @dev Function that returns both caller and contract address
     */
    function getAddresses() external view returns (address caller, address contractAddr) {
        return (msg.sender, address(this));
    }
    
    /**
     * @dev Function to receive Ether
     */
    receive() external payable {
        emit DelegateCall(msg.sender, msg.value);
    }
    
    /**
     * @dev Fallback function
     */
    fallback() external payable {
        emit DelegateCall(msg.sender, msg.value);
    }
}