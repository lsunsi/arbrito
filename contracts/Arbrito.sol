//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "./external/IAave.sol";
import "./external/IBalancer.sol";
import "./external/IERC20.sol";
import "./external/IUniswap.sol";
import "./external/IWeth.sol";

contract Arbrito is IAaveBorrower {
  address owner;
  address ethAddress;
  address wethAddress;
  address aaveAddress;
  address uniswapAddress;

  uint256 public wethInput;
  uint256 public wethMinProfit;

  constructor(
    address _ethAddress,
    address _wethAddress,
    address _aaveAddress,
    address _uniswapAddress
  ) {
    owner = msg.sender;
    ethAddress = _ethAddress;
    wethAddress = _wethAddress;
    aaveAddress = _aaveAddress;
    uniswapAddress = _uniswapAddress;

    wethInput = 10 ether;
    wethMinProfit = 0.1 ether;
  }

  receive() external payable {}

  modifier onlyOwner() {
    require(msg.sender == owner, "You're not the owner, so no.");
    _;
  }

  function setWethInput(uint256 input, uint256 minProfit) public onlyOwner {
    wethMinProfit = minProfit;
    wethInput = input;
  }

  function withdraw() public onlyOwner {
    msg.sender.transfer(address(this).balance);
  }

  function perform(address tokenAddress, address balancerAddress) public {
    address[] memory path = new address[](2);

    IUniswap uniswap = IUniswap(uniswapAddress);
    IBalancer balancer = IBalancer(balancerAddress);

    (path[0], path[1]) = (wethAddress, tokenAddress);
    uint256 tokenUniswapOutput = uniswap.getAmountsOut(wethInput, path)[1];

    uint256 tokenBalancerOutput = balancer.calcOutGivenIn(
      balancer.getBalance(wethAddress),
      balancer.getDenormalizedWeight(wethAddress),
      balancer.getBalance(tokenAddress),
      balancer.getDenormalizedWeight(tokenAddress),
      wethInput,
      balancer.getSwapFee()
    );

    uint256 wethOutput;
    address tokenSource;
    if (tokenUniswapOutput > tokenBalancerOutput) {
      tokenSource = uniswapAddress;
      wethOutput = balancer.calcOutGivenIn(
        balancer.getBalance(tokenAddress),
        balancer.getDenormalizedWeight(tokenAddress),
        balancer.getBalance(wethAddress),
        balancer.getDenormalizedWeight(wethAddress),
        tokenUniswapOutput,
        balancer.getSwapFee()
      );
    } else if (tokenBalancerOutput > tokenUniswapOutput) {
      tokenSource = balancerAddress;
      (path[0], path[1]) = (tokenAddress, wethAddress);
      wethOutput = uniswap.getAmountsOut(tokenBalancerOutput, path)[1];
    } else {
      revert("The pools agree on the price");
    }

    require(wethOutput - wethInput >= wethMinProfit, "Not worth our trouble");

    bytes memory params = abi.encode(
      tokenAddress,
      balancerAddress,
      tokenSource
    );

    address me = address(this);
    uint256 balanceBefore = me.balance;
    IAave(aaveAddress).flashLoan(me, ethAddress, wethInput, params);
    uint256 balanceAfter = me.balance;

    require(balanceAfter >= balanceBefore, "Something bad is not right");
  }

  function executeOperation(
    address reserve,
    uint256 ethInput,
    uint256 fee,
    bytes calldata params
  ) external override {
    (address tokenAddress, address balancerAddress, address tokenSource) = abi
      .decode(params, (address, address, address));

    // ETH -> WETH
    IWeth(wethAddress).deposit{ value: ethInput }();

    uint256 wethOutput;
    if (tokenSource == uniswapAddress) {
      // WETH -> TOKEN
      uint256 tokenAmount = uniswapSwap(
        address(this),
        wethAddress,
        tokenAddress,
        ethInput
      );
      // TOKEN -> WETH
      wethOutput = balancerSwap(
        balancerAddress,
        tokenAddress,
        wethAddress,
        tokenAmount
      );
    } else {
      // WETH -> TOKEN
      uint256 tokenAmount = balancerSwap(
        balancerAddress,
        wethAddress,
        tokenAddress,
        ethInput
      );
      // TOKEN -> WETH
      wethOutput = uniswapSwap(
        address(this),
        tokenAddress,
        wethAddress,
        tokenAmount
      );
    }

    // WETH -> ETH
    IWeth(wethAddress).withdraw(wethOutput);

    require(reserve == ethAddress, "Unknown reserve");
    require(payable(aaveAddress).send(ethInput + fee), "Payback failed");
  }

  function uniswapSwap(
    address me,
    address sourceToken,
    address targetToken,
    uint256 sourceAmount
  ) internal returns (uint256) {
    address[] memory path = new address[](2);
    (path[0], path[1]) = (sourceToken, targetToken);
    require(
      IERC20(sourceToken).approve(uniswapAddress, sourceAmount),
      "Uniswap weth allowance failed"
    );
    uint256 targetAmount = IUniswap(uniswapAddress).swapExactTokensForTokens(
      sourceAmount,
      0,
      path,
      me,
      block.timestamp
    )[1];
    return targetAmount;
  }

  function balancerSwap(
    address balancer,
    address sourceToken,
    address targetToken,
    uint256 sourceAmount
  ) internal returns (uint256) {
    require(
      IERC20(sourceToken).approve(balancer, sourceAmount),
      "Balancer token allowance failed"
    );
    (uint256 targetOutput, ) = IBalancer(balancer).swapExactAmountIn(
      sourceToken,
      sourceAmount,
      targetToken,
      0,
      uint256(-1)
    );
    return targetOutput;
  }
}
