//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.5;

import "./IERC20.sol";
import "../../contracts/external/IBalancer.sol";

contract BalancerPool is IBalancerPool {
  uint256 constant BONE = 10**18;

  function swapExactAmountIn(
    address _tokenIn,
    uint256 _tokenAmountIn,
    address _tokenOut,
    uint256 _minAmountOut,
    uint256 _maxPrice
  ) external override returns (uint256, uint256) {
    require(_maxPrice == uint256(-1), "Unsupported maxPrice");

    address me = address(this);
    IERC20 tokenIn = IERC20(_tokenIn);
    IERC20 tokenOut = IERC20(_tokenOut);

    uint256 tokenAmountOut =
      calcAmountOutGivenIn(_tokenAmountIn, tokenIn.balanceOf(me), tokenOut.balanceOf(me), 0);
    require(tokenAmountOut > _minAmountOut, "Insufficient amount out");

    require(tokenIn.transferFrom(msg.sender, me, _tokenAmountIn), "Transfer in failed");

    require(tokenOut.transfer(msg.sender, tokenAmountOut), "Transfer out failed");

    return (tokenAmountOut, 0);
  }

  function getBalance(address token) external view override returns (uint256) {
    return IERC20(token).balanceOf(address(this));
  }

  function bmul(uint256 a, uint256 b) internal pure returns (uint256) {
    return (a * b + BONE / 2) / BONE;
  }

  function bdiv(uint256 a, uint256 b) internal pure returns (uint256) {
    return (a * BONE + b / 2) / b;
  }

  function calcAmountOutGivenIn(
    uint256 a,
    uint256 bi,
    uint256 bo,
    uint256 s
  ) internal pure returns (uint256) {
    return bmul(bo, BONE - bdiv(bi, bi + bmul(a, BONE - s)));
  }
}
