//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

interface IBalancer {
  function getSwapFee() external view returns (uint256);

  function getDenormalizedWeight(address token) external view returns (uint256);

  function getBalance(address token) external view returns (uint256);

  function calcOutGivenIn(
    uint256 tokenBalanceIn,
    uint256 tokenWeightIn,
    uint256 tokenBalanceOut,
    uint256 tokenWeightOut,
    uint256 tokenAmountIn,
    uint256 swapFee
  ) external returns (uint256 tokenAmountOut);

  function swapExactAmountIn(
    address tokenIn,
    uint256 tokenAmountIn,
    address tokenOut,
    uint256 minAmountOut,
    uint256 maxPrice
  ) external returns (uint256 tokenAmountOut, uint256 spotPriceAfter);
}
