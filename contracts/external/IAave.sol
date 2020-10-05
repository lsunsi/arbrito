//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

interface IAave {
  function flashLoan(
    address receiver,
    address reserve,
    uint256 amount,
    bytes calldata params
  ) external;
}

interface IAaveBorrower {
  function executeOperation(
    address reserve,
    uint256 amount,
    uint256 fee,
    bytes calldata params
  ) external;
}

interface IAaveReserve {
  function transfer(address recipient, uint256 amount) external returns (bool);
}
