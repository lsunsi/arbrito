//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "./IERC20.sol";
import "../../contracts/external/IWeth.sol";

contract Weth is ERC20Mintable, IWeth {
  receive() external payable {}

  function deposit() external override payable {
    balances[msg.sender] += msg.value;
  }

  function withdraw(uint256 wad) external override {
    require(balances[msg.sender] >= wad, "Unavailable liquidity");

    balances[msg.sender] -= wad;
    require(msg.sender.send(wad), "Withdrawal transfer failed");
  }
}
