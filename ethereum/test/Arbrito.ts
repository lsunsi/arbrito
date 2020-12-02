import { expect, contract, artifacts, web3 } from "hardhat";
import { it, xit } from "mocha";

const Arbrito = artifacts.require("Arbrito");
const Uniswap = artifacts.require("Uniswap");
const Balancer = artifacts.require("Balancer");
const ERC20Mintable = artifacts.require("ERC20Mintable");
const ERC20 = artifacts.require("ERC20");

const deployContracts = async () => {
  const token0 = await ERC20Mintable.new();
  const token1 = await ERC20Mintable.new();

  const uniswap = await Uniswap.new(token0.address, token1.address);
  const balancer = await Balancer.new();

  const arbrito = await Arbrito.new();

  return [arbrito, uniswap, balancer, token0, token1];
};

contract("Arbrito", ([owner]) => {
  it("works from token0 to token1", async () => {
    const [arbrito, uniswap, balancer, token0, token1] = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("10", "ether");
    const balancerBalance1 = web3.utils.toWei("30", "ether");

    await token0.mint(uniswap.address, uniswapReserve0);
    await token1.mint(uniswap.address, uniswapReserve1);
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, balancerBalance0);
    await token1.mint(balancer.address, balancerBalance1);

    await arbrito.perform(
      0,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address,
      token0.address,
      token1.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    expect((await token0.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await token1.balanceOf(uniswap.address)).toString()).equal("11114454474534715257");

    expect((await token0.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("11", "ether")
    );
    expect((await token1.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("27", "ether")
    );

    expect((await token0.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await token1.balanceOf(arbrito.address)).toString()).equal("0");

    expect((await token0.balanceOf(owner)).toString()).equal("0");
    expect((await token1.balanceOf(owner)).toString()).equal(
      web3.utils
        .toBN(web3.utils.toWei("3", "ether"))
        .sub(web3.utils.toBN("1114454474534715257"))
        .toString()
    );
  });

  it("works from token1 to token0", async () => {
    const [arbrito, uniswap, balancer, token0, token1] = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("30", "ether");
    const balancerBalance1 = web3.utils.toWei("10", "ether");

    await token0.mint(uniswap.address, uniswapReserve0);
    await token1.mint(uniswap.address, uniswapReserve1);
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, balancerBalance0);
    await token1.mint(balancer.address, balancerBalance1);

    await arbrito.perform(
      1,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address,
      token0.address,
      token1.address,
      uniswapReserve0,
      uniswapReserve1,
      balancerBalance0
    );

    expect((await token1.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await token0.balanceOf(uniswap.address)).toString()).equal("11114454474534715257");

    expect((await token1.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("11", "ether")
    );
    expect((await token0.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("27", "ether")
    );

    expect((await token1.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await token0.balanceOf(arbrito.address)).toString()).equal("0");

    expect((await token1.balanceOf(owner)).toString()).equal("0");
    expect((await token0.balanceOf(owner)).toString()).equal(
      web3.utils
        .toBN(web3.utils.toWei("3", "ether"))
        .sub(web3.utils.toBN("1114454474534715257"))
        .toString()
    );
  });

  it("reverts if it had enough", async () => {
    const [arbrito, uniswap, balancer, token0, token1] = await deployContracts();

    const uniswapReserve0 = web3.utils.toWei("10", "ether");
    const uniswapReserve1 = web3.utils.toWei("10", "ether");

    const balancerBalance0 = web3.utils.toWei("30", "ether");
    const balancerBalance1 = web3.utils.toWei("10", "ether");

    await token0.mint(uniswap.address, uniswapReserve0);
    await token1.mint(uniswap.address, uniswapReserve1);
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, balancerBalance0);
    await token1.mint(balancer.address, balancerBalance1);

    let error;
    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("1", "ether"),
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
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
    const [arbrito, uniswap, balancer, token0, token1] = await deployContracts();

    await token0.mint(uniswap.address, 2);
    await token1.mint(uniswap.address, 2);
    await uniswap.refreshReserves();
    let count = 0;

    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
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
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
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
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
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
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
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
    const [arbrito, uniswap, balancer, token0, token1] = await deployContracts();

    let error;
    try {
      await arbrito.perform(
        0,
        web3.utils.toWei("6", "ether"),
        uniswap.address,
        balancer.address,
        token0.address,
        token1.address,
        0,
        0,
        1
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Balancer balance0 mismatch/);
  });

  xit("mainets", async () => {
    const [arbrito] = await deployContracts();

    const aave = await ERC20.at("0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9");
    const weth = await ERC20.at("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");

    await arbrito.perform(
      1,
      web3.utils.toWei("6", "ether"),
      "0xdfc14d2af169b0d36c4eff567ada9b2e0cae044f",
      "0x7c90a3cd7ec80dd2f633ed562480abbeed3be546",
      "0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9",
      "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
    );

    expect((await aave.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await weth.balanceOf(arbrito.address)).toString()).equal("0");

    expect((await aave.balanceOf(owner)).toString()).equal(
      web3.utils.toWei("0.092600543527636767", "ether")
    );
    expect((await weth.balanceOf(owner)).toString()).equal("0");
  });
});
