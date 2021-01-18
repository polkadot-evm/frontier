pragma solidity 0.5.16;

contract ExplicitRevertReason {
		function max10(uint256 a) public returns (uint256) {
			if (a > 10)
				revert("Value must not be greater than 10.");
			return a;
		}
}
