//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "../../contracts/external/IAave.sol";

contract Aave is IAave {
  address ethAddress;

  constructor(address _ethAddress) {
    ethAddress = _ethAddress;
  }

  receive() external payable {}

  function flashLoan(
    address receiver,
    address reserve,
    uint256 amount,
    bytes calldata params
  ) external override {
    require(reserve == ethAddress, "Unimplemented reserve");

    address me = address(this);

    uint256 balanceBefore = me.balance;
    uint256 fee = amount / 100;

    require(payable(receiver).send(amount), "Failed loan transfer");
    IAaveBorrower(receiver).executeOperation(reserve, amount, fee, params);

    uint256 balanceAfter = me.balance;
    require(balanceAfter == balanceBefore + fee, "Loan payback mismatch");
  }
}
