// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    function helper() internal returns (uint256) {
        return 42;
    }

    function compute() public returns (uint256) {
        return helper();
    }
}
