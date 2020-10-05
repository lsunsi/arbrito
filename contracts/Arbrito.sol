//SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.7.2;

import "./external/IAave.sol";
import "./external/IBalancer.sol";
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

    address me = address(this);
    bytes memory params = abi.encode(
      tokenAddress,
      balancerAddress,
      tokenSource,
      me
    );

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
    address[] memory path = new address[](2);

    (
      address tokenAddress,
      address balancerAddress,
      address tokenSource,
      address me
    ) = abi.decode(params, (address, address, address, address));

    IWeth(wethAddress).deposit{ value: ethInput }();

    uint256 wethOutput;
    if (tokenSource == uniswapAddress) {
      (path[0], path[1]) = (wethAddress, tokenAddress);
      uint256 tokenAmount = IUniswap(uniswapAddress).swapExactTokensForTokens(
        ethInput,
        0,
        path,
        me,
        block.timestamp
      )[1];
      (wethOutput, ) = IBalancer(balancerAddress).swapExactAmountIn(
        tokenAddress,
        tokenAmount,
        wethAddress,
        0,
        uint256(-1)
      );
    } else {
      (uint256 tokenAmount, ) = IBalancer(balancerAddress).swapExactAmountIn(
        wethAddress,
        ethInput,
        tokenAddress,
        0,
        uint256(-1)
      );
      (path[0], path[1]) = (tokenAddress, wethAddress);
      wethOutput = IUniswap(uniswapAddress).swapExactTokensForTokens(
        tokenAmount,
        0,
        path,
        me,
        block.timestamp
      )[1];
    }

    IWeth(wethAddress).withdraw(wethOutput);

    require(reserve == ethAddress, "Unknown reserve");
    require(payable(aaveAddress).send(ethInput + fee), "Payback failed");
  }
}
