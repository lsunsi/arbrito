//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "./IERC20.sol";
import "../../contracts/external/IBalancer.sol";

contract Balancer is IBalancer {
  address linkAddress;
  address wethAddress;

  uint256 swapFee = 0;
  uint256 weight = 1;

  constructor(address _linkAddress, address _wethAddress) {
    linkAddress = _linkAddress;
    wethAddress = _wethAddress;
  }

  function getSwapFee() external override view returns (uint256) {
    return swapFee;
  }

  function getDenormalizedWeight(address)
    external
    override
    view
    returns (uint256)
  {
    return weight;
  }

  function getBalance(address token) external override view returns (uint256) {
    return IERC20(token).balanceOf(address(this));
  }

  function calcOutGivenIn(
    uint256 tokenBalanceIn,
    uint256 tokenWeightIn,
    uint256 tokenBalanceOut,
    uint256 tokenWeightOut,
    uint256 tokenAmountIn,
    uint256 _swapFee
  ) external override returns (uint256) {
    require(swapFee == _swapFee, "Mismatched swap fees");
    swapFee = swapFee; // shhhh

    return
      calcOutGivenInInternal(
        tokenBalanceIn,
        tokenWeightIn,
        tokenBalanceOut,
        tokenWeightOut,
        tokenAmountIn,
        _swapFee
      );
  }

  function swapExactAmountIn(
    address tokenIn,
    uint256 tokenAmountIn,
    address tokenOut,
    uint256 minAmountOut,
    uint256 maxPrice
  ) external override returns (uint256, uint256) {
    require(minAmountOut == 0, "Unsupported minAmountOut");
    require(maxPrice == uint256(-1), "Unsupported maxPrice");

    address me = address(this);
    IERC20 tokenIn20 = IERC20(tokenIn);
    IERC20 tokenOut20 = IERC20(tokenOut);

    uint256 amountOut = calcOutGivenInInternal(
      tokenIn20.balanceOf(me),
      weight,
      tokenOut20.balanceOf(me),
      weight,
      tokenAmountIn,
      swapFee
    );

    require(
      tokenIn20.transferFrom(msg.sender, me, tokenAmountIn),
      "Transfer in failed"
    );

    require(IERC20(tokenOut).approve(me, amountOut), "Approval failed");

    require(
      tokenOut20.transferFrom(me, msg.sender, amountOut),
      "Transfer out failed"
    );

    return (amountOut, 0);
  }

  function calcOutGivenInInternal(
    uint256 tokenBalanceIn,
    uint256 tokenWeightIn,
    uint256 tokenBalanceOut,
    uint256 tokenWeightOut,
    uint256 tokenAmountIn,
    uint256 _swapFee
  ) internal pure returns (uint256) {
    require(tokenWeightIn == tokenWeightOut, "Mismatched weights");
    require(_swapFee == 0, "Mismatched swap fees");

    uint256 tokenAmountOut = 10**18;
    tokenAmountOut *= (tokenBalanceOut * tokenAmountIn);
    tokenAmountOut /= tokenBalanceIn;

    return tokenAmountOut / 10**18;
  }
}
