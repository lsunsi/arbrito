//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

interface IERC20 {
  function transferFrom(
    address sender,
    address recipient,
    uint256 amount
  ) external returns (bool);

  function balanceOf(address account) external view returns (uint256);
}

contract ERC20 is IERC20 {
  mapping(address => uint256) balances;

  function balanceOf(address account) external override view returns (uint256) {
    return balances[account];
  }

  function transferFrom(
    address sender,
    address recipient,
    uint256 amount
  ) external override returns (bool) {
    if (balances[sender] < amount) {
      return false;
    }

    balances[sender] -= amount;
    balances[recipient] += amount;
    return true;
  }
}

contract ERC20Mintable is ERC20 {
  function mint(address account, uint256 amount) external {
    balances[account] += amount;
  }
}
