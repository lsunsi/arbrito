//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.5;

import "./external/IBalancer.sol";
import "./external/IUniswap.sol";
import "./external/IERC20.sol";

contract Arbrito is IUniswapPairCallee {
  function perform(
    bool direction,
    uint256 amount,
    address uniswapPair,
    address balancerPool,
    uint256 blockNumber,
    uint256 uniswapReserve0,
    uint256 uniswapReserve1
  ) external {
    require(block.number == blockNumber, "Delayed execution");
    (uint256 amount0, uint256 amount1) = direction ? (amount, uint256(0)) : (uint256(0), amount);

    (uint256 reserve0, uint256 reserve1, ) = IUniswapPair(uniswapPair).getReserves();
    require(reserve0 == uniswapReserve0, "Reserve0 mismatch");
    require(reserve1 == uniswapReserve1, "Reserve1 mismatch");

    bytes memory payload = abi.encode(balancerPool, msg.sender, reserve0, reserve1);
    IUniswapPair(uniswapPair).swap(amount0, amount1, address(this), payload);
  }

  function uniswapV2Call(
    address, // sender
    uint256 amount0,
    uint256 amount1,
    bytes calldata data
  ) external override {
    (address balancerPoolAddress, address ownerAddress, uint256 reserve0, uint256 reserve1) =
      abi.decode(data, (address, address, uint256, uint256));

    IUniswapPair uniswapPair = IUniswapPair(msg.sender);

    uint256 amountTrade;
    uint256 amountPayback;

    uint256 reservePayback;
    uint256 reserveTrade;

    address tokenPayback;
    address tokenTrade;

    if (amount0 != 0) {
      (reserveTrade, reservePayback) = (reserve0, reserve1);
      tokenPayback = uniswapPair.token1();
      tokenTrade = uniswapPair.token0();
      amountTrade = amount0;
    } else {
      (reservePayback, reserveTrade) = (reserve0, reserve1);
      tokenPayback = uniswapPair.token0();
      tokenTrade = uniswapPair.token1();
      amountTrade = amount1;
    }

    amountPayback = calculateUniswapPayback(amountTrade, reservePayback, reserveTrade);

    IERC20(tokenTrade).approve(balancerPoolAddress, amountTrade);

    (uint256 balancerAmountOut, ) =
      IBalancerPool(balancerPoolAddress).swapExactAmountIn(
        tokenTrade,
        amountTrade,
        tokenPayback,
        amountPayback,
        uint256(-1)
      );

    require(IERC20(tokenPayback).transfer(msg.sender, amountPayback), "Payback failed");

    require(
      IERC20(tokenPayback).transfer(ownerAddress, balancerAmountOut - amountPayback),
      "Sender transfer failed"
    );
  }

  function calculateUniswapPayback(
    uint256 amountOut,
    uint256 reserveIn,
    uint256 reserveOut
  ) internal pure returns (uint256) {
    uint256 numerator = reserveIn * amountOut * 1000;
    uint256 denominator = (reserveOut - amountOut) * 997;
    return numerator / denominator + 1;
  }
}
