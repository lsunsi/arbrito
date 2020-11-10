//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.4;

import "./IERC20.sol";
import "../../contracts/external/IUniswap.sol";

contract Uniswap is IUniswapPair {
  address token0address;
  address token1address;

  constructor(address _token0, address _token1) {
    token0address = _token0;
    token1address = _token1;
  }

  function token0() external view override returns (address) {
    return token0address;
  }

  function token1() external view override returns (address) {
    return token1address;
  }

  function getReserves()
    external
    view
    override
    returns (
      uint112,
      uint112,
      uint32
    )
  {
    address me = address(this);
    return (
      uint112(IERC20(token0address).balanceOf(me)),
      uint112(IERC20(token1address).balanceOf(me)),
      0
    );
  }

  function swap(
    uint256 amount0,
    uint256 amount1,
    address receiver,
    bytes calldata payload
  ) external override {
    require(payload.length != 0, "Unsupported payload");
    require(
      (amount0 == 0 && amount1 != 0) || (amount0 != 0 && amount1 == 0),
      "Unsupported amounts"
    );

    IERC20 tokenLent;
    IERC20 tokenPayback;
    uint256 amountLent;

    if (amount0 != 0) {
      tokenLent = IERC20(token0address);
      tokenPayback = IERC20(token1address);
      amountLent = amount0;
      require(IERC20(token0address).transfer(receiver, amount0), "loan failed");
    } else {
      tokenLent = IERC20(token1address);
      tokenPayback = IERC20(token0address);
      amountLent = amount1;
      require(IERC20(token1address).transfer(receiver, amount1), "loan failed");
    }

    address me = address(this);
    uint256 tokenLentBalance = tokenLent.balanceOf(me);
    uint256 tokenPaybackBalance = tokenPayback.balanceOf(me);

    IUniswapPairCallee(msg.sender).uniswapV2Call(
      msg.sender,
      amount0,
      amount1,
      payload
    );

    require(tokenLent.balanceOf(me) == tokenLentBalance, "unsupported payback");

    uint256 tokenPaybackBalanceAfter = tokenPayback.balanceOf(me);
    require(tokenPaybackBalanceAfter > tokenPaybackBalance, "missing payback");

    uint256 amountPaidBack = tokenPaybackBalanceAfter - tokenPaybackBalance;
    uint256 balance0Adjusted = tokenPaybackBalanceAfter *
      1000 -
      amountPaidBack *
      3;
    uint256 balance1Adjusted = tokenLentBalance;
    require(
      balance0Adjusted * balance1Adjusted >=
        (tokenLentBalance + amountLent) * tokenPaybackBalance * 1000,
      "payback mismatch"
    );
  }
}
