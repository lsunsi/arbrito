//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.5;

import "./IERC20.sol";
import "../../contracts/external/IUniswap.sol";

contract UniswapPair is IUniswapPair {
  address token0address;
  address token1address;
  uint112 reserve0;
  uint112 reserve1;

  constructor(address _token0, address _token1) {
    token0address = _token0;
    token1address = _token1;
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
    return (reserve0, reserve1, 0);
  }

  function refreshReserves() public {
    address me = address(this);
    reserve0 = uint112(IERC20(token0address).balanceOf(me));
    reserve1 = uint112(IERC20(token1address).balanceOf(me));
  }

  function swap(
    uint256 amount0,
    uint256 amount1,
    address receiver,
    bytes calldata payload
  ) external override {
    require(
      (amount0 == 0 && amount1 != 0) || (amount0 != 0 && amount1 == 0),
      "unsupported amounts"
    );

    IERC20 tokenLent;
    IERC20 tokenPayback;
    uint256 amountLent;
    uint256 tokenPaybackBalance;

    if (amount0 != 0) {
      tokenLent = IERC20(token0address);
      tokenPayback = IERC20(token1address);
      amountLent = amount0;
      tokenPaybackBalance = reserve1;
      require(IERC20(token0address).transfer(receiver, amount0), "loan failed");
    } else {
      tokenLent = IERC20(token1address);
      tokenPayback = IERC20(token0address);
      amountLent = amount1;
      tokenPaybackBalance = reserve0;
      require(IERC20(token1address).transfer(receiver, amount1), "loan failed");
    }

    address me = address(this);
    uint256 tokenLentBalance = tokenLent.balanceOf(me);

    if (payload.length > 0) {
      IUniswapPairCallee(msg.sender).uniswapV2Call(msg.sender, amount0, amount1, payload);
    }

    require(tokenLent.balanceOf(me) == tokenLentBalance, "unsupported payback");

    uint256 tokenPaybackBalanceAfter = tokenPayback.balanceOf(me);
    require(tokenPaybackBalanceAfter > tokenPaybackBalance, "missing payback");

    uint256 amountPaidBack = tokenPaybackBalanceAfter - tokenPaybackBalance;
    uint256 balance0Adjusted = tokenPaybackBalanceAfter * 1000 - amountPaidBack * 3;
    uint256 balance1Adjusted = tokenLentBalance;
    require(
      balance0Adjusted * balance1Adjusted >=
        (tokenLentBalance + amountLent) * tokenPaybackBalance * 1000,
      "payback mismatch"
    );

    refreshReserves();
  }
}

contract UniswapRouter is IUniswapRouter {
  address immutable weth;
  address immutable token;
  address immutable pair;

  constructor(
    address _weth,
    address _token,
    address _pair
  ) {
    weth = _weth;
    token = _token;
    pair = _pair;
  }

  function swapExactTokensForTokens(
    uint256 amountIn,
    uint256 amountOutMin,
    address[] calldata path,
    address to,
    uint256 deadline
  ) external override returns (uint256[] memory) {
    require(amountOutMin == 0, "amountOutMin must be 0");
    require(deadline == block.timestamp, "deadline must be block.timestamp");
    require(path.length == 2, "path.length must be 2");

    uint256[] memory amounts = new uint256[](2);
    amounts[0] = amountIn;

    (uint112 reserve0, uint112 reserve1, ) = UniswapPair(pair).getReserves();

    if (path[0] == weth && path[1] == token) {
      amounts[1] = getAmountOut(amountIn, reserve0, reserve1);

      IERC20(weth).transferFrom(msg.sender, pair, amountIn);
      UniswapPair(pair).swap(0, amounts[1], to, new bytes(0));
    } else if (path[0] == token && path[1] == weth) {
      amounts[1] = getAmountOut(amountIn, reserve1, reserve0);

      IERC20(token).transferFrom(msg.sender, pair, amountIn);
      UniswapPair(pair).swap(amounts[1], 0, to, new bytes(0));
    } else {
      revert("invalid path");
    }

    return amounts;
  }

  function getAmountOut(
    uint256 amountIn,
    uint112 reserveIn,
    uint112 reserveOut
  ) internal pure returns (uint256) {
    return (amountIn * reserveOut * 997) / (reserveIn * 1000 + amountIn * 997);
  }
}
