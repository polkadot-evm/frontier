// SPDX-License-Identifier: GPL-3.0-only
pragma solidity ^0.8.2;

contract StorageGrowthTest {
	mapping(uint256 => uint256) public map;
	uint256 foo;
	uint256 bar;
	uint256 baz;

	constructor() {
		foo = 1;
		bar = 2;
		baz = 3;
	}

	function store() public {
		map[0] = 1;
		map[1] = 2;
		map[2] = 3;
	}

	function update() public {
		foo = 2;
		bar = 3;
		baz = 4;
	}
}
