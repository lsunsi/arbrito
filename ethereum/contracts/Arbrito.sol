//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.5;

import "./external/IBalancer.sol";
import "./external/IUniswap.sol";
import "./external/IERC20.sol";
import "./external/IWeth.sol";

contract Arbrito is IUniswapPairCallee {
  enum Borrow { Token0, Token1 }

  address[] public tokens;
  mapping(address => uint256) public balances;

  address immutable WETH_ADDRESS;
  address immutable UNISWAP_ROUTER_ADDRESS;
  address payable immutable OWNER;

  constructor(address weth_address, address uniswap_router_address) {
    UNISWAP_ROUTER_ADDRESS = uniswap_router_address;
    WETH_ADDRESS = weth_address;
    OWNER = msg.sender;
  }

  function perform(
    Borrow borrow,
    uint256 amount,
    address uniswapPair,
    address balancerPool,
    address uniswapToken0,
    address uniswapToken1,
    uint256 uniswapReserve0,
    uint256 uniswapReserve1,
    uint256 balancerBalance0
  ) external {
    (uint256 reserve0, uint256 reserve1, ) = IUniswapPair(uniswapPair).getReserves();

    require(
      borrow == Borrow.Token0
        ? (reserve0 >= uniswapReserve0 && reserve1 <= uniswapReserve1)
        : (reserve0 <= uniswapReserve0 && reserve1 >= uniswapReserve1),
      "Uniswap reserves mismatch"
    );

    require(
      IBalancerPool(balancerPool).getBalance(uniswapToken0) == balancerBalance0,
      "Balancer balance0 mismatch"
    );

    bytes memory payload =
      abi.encode(balancerPool, uniswapToken0, uniswapToken1, uniswapReserve0, uniswapReserve1);

    if (borrow == Borrow.Token0) {
      IUniswapPair(uniswapPair).swap(amount, 0, address(this), payload);
    } else {
      IUniswapPair(uniswapPair).swap(0, amount, address(this), payload);
    }
  }

  function uniswapV2Call(
    address sender,
    uint256 amount0,
    uint256 amount1,
    bytes calldata data
  ) external override {
    (
      address balancerPoolAddress,
      address token0,
      address token1,
      uint256 reserve0,
      uint256 reserve1
    ) = abi.decode(data, (address, address, address, uint256, uint256));

    uint256 amountTrade;
    uint256 amountPayback;

    address tokenPayback;
    address tokenTrade;

    if (amount0 != 0) {
      amountTrade = amount0;
      (tokenTrade, tokenPayback) = (token0, token1);
      amountPayback = calculateUniswapPayback(amountTrade, reserve1, reserve0);
    } else {
      amountTrade = amount1;
      (tokenPayback, tokenTrade) = (token0, token1);
      amountPayback = calculateUniswapPayback(amountTrade, reserve0, reserve1);
    }

    allow(sender, balancerPoolAddress, tokenTrade, amountTrade);

    (uint256 balancerAmountOut, ) =
      IBalancerPool(balancerPoolAddress).swapExactAmountIn(
        tokenTrade,
        amountTrade,
        tokenPayback,
        amountPayback,
        uint256(-1)
      );

    require(IERC20(tokenPayback).transfer(msg.sender, amountPayback), "Payback failed");

    if (balances[tokenPayback] == 0) {
      tokens.push(tokenPayback);
    }

    balances[tokenPayback] += balancerAmountOut - amountPayback;
  }

  function allow(
    address sender,
    address balancer,
    address tokenTrade,
    uint256 amountTrade
  ) internal {
    if (IERC20(tokenTrade).allowance(sender, balancer) < amountTrade) {
      IERC20(tokenTrade).approve(balancer, uint256(-1));
    }
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

  function withdraw() external {
    address[] memory path = new address[](2);
    address me = address(this);
    path[1] = WETH_ADDRESS;

    uint256 weth = 0;
    for (uint256 i = 0; i < tokens.length; i++) {
      address token = tokens[i];
      path[0] = token;

      weth += IUniswapRouter(UNISWAP_ROUTER_ADDRESS).swapExactTokensForTokens(
        balances[token],
        0,
        path,
        me,
        block.timestamp
      )[1];

      delete balances[token];
    }

    delete tokens;

    IWeth(WETH_ADDRESS).withdraw(weth);
    OWNER.transfer(weth);
  }
}
