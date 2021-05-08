pragma solidity ^0.8.2;

contract Lock {
    // address owner; slot #0
    // address unlockTime; slot #1
    constructor (address owner, uint256 unlockTime) public payable {
        assembly {
            sstore(0x00, owner)
            sstore(0x01, unlockTime)
        }
    }
    
    /**
     * @dev        Withdraw function once timestamp has passed unlock time
     */
    fallback () external payable { // payable so solidity doesn't add unnecessary logic
        assembly {
            switch gt(timestamp(), sload(0x01))
            case 0 { revert(0, 0) }
            case 1 {
                switch call(gas(), sload(0x00), balance(address()), 0, 0, 0, 0)
                case 0 { revert(0, 0) }
            }
        }
    }
}

contract Lockdrop {
    enum Term {
        ThreeMo,
        SixMo,
        TwelveMo
    }
    // Time constants
    uint256 constant public LOCK_DROP_PERIOD = 1 days * 92; // 3 months
    uint256 public LOCK_START_TIME;
    uint256 public LOCK_END_TIME;
    // ETH locking events
    event Locked(address indexed owner, uint256 eth, Lock lockAddr, Term term, bytes edgewareAddr, bool isValidator, uint time);
    event Signaled(address indexed contractAddr, bytes edgewareAddr, uint time);
    
    constructor(uint startTime) public {
        LOCK_START_TIME = startTime;
        LOCK_END_TIME = startTime + LOCK_DROP_PERIOD;
    }

    /**
     * @dev        Locks up the value sent to contract in a new Lock
     * @param      term         The length of the lock up
     * @param      edgewareAddr The bytes representation of the target edgeware key
     * @param      isValidator  Indicates if sender wishes to be a validator
     */
    function lock(Term term, bytes calldata edgewareAddr, bool isValidator)
        external
        payable
        // didStart
        // didNotEnd
    {
        uint256 eth = msg.value;
        address owner = msg.sender;
        uint256 unlockTime = unlockTimeForTerm(term);
        // Create ETH lock contract
        Lock lockAddr = (new Lock){value: eth}(owner, unlockTime);
        // ensure lock contract has at least all the ETH, or fail
        assert(address(lockAddr).balance >= msg.value);
        emit Locked(owner, eth, lockAddr, term, edgewareAddr, isValidator, block.timestamp);
    }

    /**
     * @dev        Signals a contract's (or address's) balance decided after lock period
     * @param      contractAddr  The contract address from which to signal the balance of
     * @param      nonce         The transaction nonce of the creator of the contract
     * @param      edgewareAddr   The bytes representation of the target edgeware key
     */
    function signal(address contractAddr, uint32 nonce, bytes calldata edgewareAddr)
        external
        didStart
        didNotEnd
        didCreate(contractAddr, msg.sender, nonce)
    {
        emit Signaled(contractAddr, edgewareAddr, block.timestamp);
    }

    function unlockTimeForTerm(Term term) internal view returns (uint256) {
        if (term == Term.ThreeMo) return block.timestamp + 92 days;
        if (term == Term.SixMo) return block.timestamp + 183 days;
        if (term == Term.TwelveMo) return block.timestamp + 365 days;
        
        revert();
    }

    /**
     * @dev        Ensures the lockdrop has started
     */
    modifier didStart() {
        require(block.timestamp >= LOCK_START_TIME);
        _;
    }

    /**
     * @dev        Ensures the lockdrop has not ended
     */
    modifier didNotEnd() {
        require(block.timestamp <= LOCK_END_TIME);
        _;
    }

    /**
     * @dev        Rebuilds the contract address from a normal address and transaction nonce
     * @param      _origin  The non-contract address derived from a user's public key
     * @param      _nonce   The transaction nonce from which to generate a contract address
     */
    function addressFrom(address _origin, uint32 _nonce) public pure returns (address) {
        if(_nonce == 0x00)     return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xd6), bytes1(0x94), _origin, bytes1(0x80))))));
        if(_nonce <= 0x7f)     return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xd6), bytes1(0x94), _origin, uint8(_nonce))))));
        if(_nonce <= 0xff)     return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xd7), bytes1(0x94), _origin, bytes1(0x81), uint8(_nonce))))));
        if(_nonce <= 0xffff)   return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xd8), bytes1(0x94), _origin, bytes1(0x82), uint16(_nonce))))));
        if(_nonce <= 0xffffff) return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xd9), bytes1(0x94), _origin, bytes1(0x83), uint24(_nonce))))));
        return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xda), bytes1(0x94), _origin, bytes1(0x84), uint32(_nonce)))))); // more than 2^32 nonces not realistic
    }

    /**
     * @dev        Ensures the target address was created by a parent at some nonce
     * @param      target  The target contract address (or trivially the parent)
     * @param      parent  The creator of the alleged contract address
     * @param      nonce   The creator's tx nonce at the time of the contract creation
     */
    modifier didCreate(address target, address parent, uint32 nonce) {
        // Trivially let senders "create" themselves
        if (target == parent) {
            _;
        } else {
            require(target == addressFrom(parent, nonce));
            _;
        }
    }
}
