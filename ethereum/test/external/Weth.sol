//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.5;

import "../../contracts/external/IWeth.sol";
import "./IERC20.sol";

contract Weth is ERC20Mintable, IWeth {
  receive() external payable {
    balances[msg.sender] += msg.value;
  }

  function withdraw(uint256 wad) external override {
    require(balances[msg.sender] >= wad, "insufficient amount");
    balances[msg.sender] -= wad;
    msg.sender.transfer(wad);
  }
}
