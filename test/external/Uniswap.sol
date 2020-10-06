//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "./IERC20.sol";
import "../../contracts/external/IUniswap.sol";

contract Uniswap is IUniswap {
  address linkAddress;
  address wethAddress;

  constructor(address _linkAddress, address _wethAddress) {
    linkAddress = _linkAddress;
    wethAddress = _wethAddress;
  }

  function getAmountsOut(uint256 amountIn, address[] memory path)
    external
    override
    view
    returns (uint256[] memory)
  {
    return getAmountsOutInternal(amountIn, path);
  }

  function swapExactTokensForTokens(
    uint256 amountIn,
    uint256 amountOutMin,
    address[] calldata path,
    address to,
    uint256 deadline
  ) external override returns (uint256[] memory) {
    require(amountOutMin == 0, "Unsupported amountOutMin");
    require(deadline == block.timestamp, "Unsupported deadline");

    address me = address(this);
    uint256[] memory amounts = getAmountsOutInternal(amountIn, path);

    require(
      IERC20(path[0]).transferFrom(msg.sender, me, amountIn),
      "Transfer in failed"
    );

    require(IERC20(path[1]).approve(me, amounts[1]), "Approval failed");

    require(
      IERC20(path[1]).transferFrom(me, to, amounts[1]),
      "Transfer out failed"
    );

    return amounts;
  }

  function getAmountsOutInternal(uint256 amountIn, address[] memory path)
    internal
    view
    returns (uint256[] memory)
  {
    address me = address(this);

    uint256[] memory amounts = new uint256[](2);

    uint256 amountOut = 10**18;
    amountOut *= (amountIn * IERC20(path[1]).balanceOf(me));
    amountOut /= IERC20(path[0]).balanceOf(me);

    amounts[0] = amountIn;
    amounts[1] = amountOut / 10**18;

    return amounts;
  }
}
