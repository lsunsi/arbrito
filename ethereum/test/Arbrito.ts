import { expect, contract, artifacts, web3 } from "hardhat";
import { Address } from "hardhat-deploy/dist/types";
import { it, xit } from "mocha";
import deploy from "../deploy/mainet";

const Arbrito = artifacts.require("Arbrito");
const UniswapPair = artifacts.require("UniswapPair");
const UniswapRouter = artifacts.require("UniswapRouter");
const BalancerPool = artifacts.require("BalancerPool");
const ERC20Mintable = artifacts.require("ERC20Mintable");
const ERC20 = artifacts.require("ERC20");
const Weth = artifacts.require("Weth");

const deployContracts = async () => {
  const weth = await Weth.new();

  const token = await ERC20Mintable.new();

  const uniswapPair = await UniswapPair.new(weth.address, token.address);
  const uniswapRouter = await UniswapRouter.new(weth.address, token.address, uniswapPair.address);
  const balancerPool = await BalancerPool.new();

  const arbrito = await Arbrito.new(weth.address, uniswapRouter.address);

  return { arbrito, uniswapPair, uniswapRouter, balancerPool, weth, token };
};

contract("Arbrito", ([owner, other]) => {
  it("works from weth to token", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("10", "ether");
    const balancerBalance1 = web3.utils.toWei("30", "ether");

    await weth.mint(uniswapPair.address, uniswapReserve0);
    await token.mint(uniswapPair.address, uniswapReserve1);
    await uniswapPair.refreshReserves();

    await weth.mint(balancerPool.address, balancerBalance0);
    await token.mint(balancerPool.address, balancerBalance1);

    await arbrito.perform(
      0,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    expect((await weth.balanceOf(uniswapPair.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await token.balanceOf(uniswapPair.address)).toString()).equal("11114454474534715257");

    expect((await weth.balanceOf(balancerPool.address)).toString()).equal(
      web3.utils.toWei("11", "ether")
    );
    expect((await token.balanceOf(balancerPool.address)).toString()).equal("27272727272727272730");

    expect((await weth.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await token.balanceOf(arbrito.address)).toString()).equal(
      web3.utils.toBN("2727272727272727270").sub(web3.utils.toBN("1114454474534715257")).toString()
    );
  });

  it("works from token to weth", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("30", "ether");
    const balancerBalance1 = web3.utils.toWei("10", "ether");

    await weth.mint(uniswapPair.address, uniswapReserve0);
    await token.mint(uniswapPair.address, uniswapReserve1);
    await uniswapPair.refreshReserves();

    await weth.mint(balancerPool.address, balancerBalance0);
    await token.mint(balancerPool.address, balancerBalance1);

    await arbrito.perform(
      1,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    expect((await token.balanceOf(uniswapPair.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await weth.balanceOf(uniswapPair.address)).toString()).equal("11114454474534715257");

    expect((await token.balanceOf(balancerPool.address)).toString()).equal(
      web3.utils.toWei("11", "ether")
    );
    expect((await weth.balanceOf(balancerPool.address)).toString()).equal("27272727272727272730");

    expect((await token.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await weth.balanceOf(arbrito.address)).toString()).equal(
      web3.utils.toBN("2727272727272727270").sub(web3.utils.toBN("1114454474534715257")).toString()
    );
  });

  it("reverts if it had enough", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("30", "ether");
    const balancerBalance1 = web3.utils.toWei("10", "ether");

    await weth.mint(uniswapPair.address, uniswapReserve0);
    await token.mint(uniswapPair.address, uniswapReserve1);
    await uniswapPair.refreshReserves();

    await weth.mint(balancerPool.address, balancerBalance0);
    await token.mint(balancerPool.address, balancerBalance1);

    let error;
    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("1", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        uniswapReserve0,
        uniswapReserve1,
        balancerBalance0
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Insufficient amount out/);
  });

  it("reverts if the uniswap reserves are worse than expected", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    await weth.mint(uniswapPair.address, 2);
    await token.mint(uniswapPair.address, 2);
    await uniswapPair.refreshReserves();
    let count = 0;

    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        3,
        1,
        0
      );
    } catch (e) {
      expect(e).match(/Uniswap reserves mismatch/);
      count++;
    }

    try {
      await arbrito.perform(
        1,
        web3.utils.toWei("6", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        1,
        3,
        0
      );
    } catch (e) {
      expect(e).match(/Uniswap reserves mismatch/);
      count++;
    }

    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        1,
        1,
        0
      );
    } catch (e) {
      expect(e).match(/Uniswap reserves mismatch/);
      count++;
    }

    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        3,
        3,
        0
      );
    } catch (e) {
      expect(e).match(/Uniswap reserves mismatch/);
      count++;
    }

    expect(count).eq(4);
  });

  it("reverts if the balancer balance0 is different than expected", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    let error;
    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswapPair.address,
        balancerPool.address,
        weth.address,
        token.address,
        0,
        0,
        1
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Balancer balance0 mismatch/);
  });

  it("increase allowance only when needed", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    expect((await weth.allowance(arbrito.address, balancerPool.address)).toString()).eq("0");

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("10", "ether");
    const balancerBalance1 = web3.utils.toWei("30", "ether");

    await weth.mint(uniswapPair.address, uniswapReserve0);
    await token.mint(uniswapPair.address, uniswapReserve1);
    await uniswapPair.refreshReserves();

    await weth.mint(balancerPool.address, balancerBalance0);
    await token.mint(balancerPool.address, balancerBalance1);

    await arbrito.perform(
      0,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    let wethdiff = web3.utils.toBN(uniswapReserve0).sub(await weth.balanceOf(uniswapPair.address));
    let allowance = web3.utils
      .toBN("115792089237316195423570985008687907853269984665640564039457584007913129639935")
      .sub(wethdiff);

    expect((await weth.allowance(arbrito.address, balancerPool.address)).toString()).equal(
      allowance.toString()
    );

    await weth.mint(uniswapPair.address, wethdiff);
    await uniswapPair.refreshReserves();

    await arbrito.perform(
      0,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      uniswapReserve0,
      await token.balanceOf(uniswapPair.address),
      await weth.balanceOf(balancerPool.address)
    );

    expect((await weth.allowance(arbrito.address, balancerPool.address)).toString()).equal(
      allowance
        .sub(web3.utils.toBN(uniswapReserve0).sub(await weth.balanceOf(uniswapPair.address)))
        .toString()
    );
  });

  it("withdraws our moni", async () => {
    const { arbrito, uniswapPair, balancerPool, weth, token } = await deployContracts();

    web3.eth.sendTransaction({
      from: owner,
      to: weth.address,
      value: web3.utils.toWei("50", "ether"),
    });

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("30", "ether");
    const balancerBalance1 = web3.utils.toWei("10", "ether");

    await weth.mint(uniswapPair.address, uniswapReserve0);
    await token.mint(uniswapPair.address, uniswapReserve1);
    await uniswapPair.refreshReserves();

    await weth.mint(balancerPool.address, balancerBalance0);
    await token.mint(balancerPool.address, balancerBalance1);

    await arbrito.perform(
      1,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    expect((await weth.balanceOf(arbrito.address)).toString()).eq("1612818252738012013");

    await token.mint(balancerPool.address, web3.utils.toWei("70", "ether"));

    await arbrito.perform(
      0,
      web3.utils.toWei("1", "ether"),
      uniswapPair.address,
      balancerPool.address,
      weth.address,
      token.address,
      await weth.balanceOf(uniswapPair.address),
      await token.balanceOf(uniswapPair.address),
      await weth.balanceOf(balancerPool.address)
    );

    expect((await token.balanceOf(arbrito.address)).toString()).eq("1972458627463811272");

    const ownerBalance1 = web3.utils.toBN(await web3.eth.getBalance(owner));

    await arbrito.withdraw({ from: other });

    const ownerBalance2 = web3.utils.toBN(await web3.eth.getBalance(owner));

    expect((await weth.balanceOf(arbrito.address)).toString()).eq("0");
    expect((await token.balanceOf(arbrito.address)).toString()).eq("0");
    expect(ownerBalance2.sub(ownerBalance1).toString()).eq("3290062057159031070");

    await arbrito.withdraw({ from: other });

    const ownerBalance3 = web3.utils.toBN(await web3.eth.getBalance(owner));
    expect(ownerBalance3.eq(ownerBalance2)).eq(true);
  });

  xit("mainets", async () => {
    const arbrito = await Arbrito.new(
      "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
      "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
    );

    const token0 = await ERC20.at("0xc011a73ee8576fb46f5e1c5751ca3b9fe0af2a6f");
    const token1 = await ERC20.at("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");

    const uniswapPairAddress = "0x43ae24960e5534731fc831386c07755a2dc33d47";
    const balancerPoolAddress = "0x1373e57f764a7944bdd7a4bd5ca3007d496934da";
    const uniswapReserve0 = "711050248732125337962178";
    const uniswapReserve1 = "6416406142955321706189";
    const balancerBalance0 = "2011899504160281443201";

    await arbrito.perform(
      1,
      web3.utils.toWei("0.217044074567221250", "ether"),
      uniswapPairAddress,
      balancerPoolAddress,
      token0.address,
      token1.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    const ownerBalance1 = web3.utils.toBN(await web3.eth.getBalance(owner));
    await arbrito.withdraw({ from: other });
    const ownerBalance2 = web3.utils.toBN(await web3.eth.getBalance(owner));

    expect((await token0.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await token1.balanceOf(arbrito.address)).toString()).equal("0");

    expect(ownerBalance2.sub(ownerBalance1).toString()).equal("2674630331710351");
  });
});
