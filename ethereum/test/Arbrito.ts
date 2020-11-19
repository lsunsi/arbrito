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

    await token0.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await token1.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, web3.utils.toWei("10", "ether"));
    await token1.mint(balancer.address, web3.utils.toWei("30", "ether"));

    await arbrito.perform(
      true,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address,
      (await web3.eth.getBlockNumber()) + 1
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

    await token0.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await token1.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, web3.utils.toWei("30", "ether"));
    await token1.mint(balancer.address, web3.utils.toWei("10", "ether"));

    await arbrito.perform(
      false,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address,
      (await web3.eth.getBlockNumber()) + 1
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

    await token0.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await token1.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, web3.utils.toWei("30", "ether"));
    await token1.mint(balancer.address, web3.utils.toWei("10", "ether"));

    let error;
    try {
      await arbrito.perform(
        true,
        web3.utils.toWei("1", "ether"),
        uniswap.address,
        balancer.address,
        (await web3.eth.getBlockNumber()) + 1
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Insufficient amount out/);
  });

  it("reverts when the block is mined in delayed block", async () => {
    const [arbrito] = await deployContracts();

    let error;
    try {
      await arbrito.perform(
        false,
        web3.utils.toWei("6", "ether"),
        "0xdfc14d2af169b0d36c4eff567ada9b2e0cae044f",
        "0x7c90a3cd7ec80dd2f633ed562480abbeed3be546",
        await web3.eth.getBlockNumber()
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Delayed execution/);
  });

  xit("mainets", async () => {
    const [arbrito] = await deployContracts();

    const aave = await ERC20.at("0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9");
    const weth = await ERC20.at("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");

    await arbrito.perform(
      false,
      web3.utils.toWei("6", "ether"),
      "0xdfc14d2af169b0d36c4eff567ada9b2e0cae044f",
      "0x7c90a3cd7ec80dd2f633ed562480abbeed3be546",
      (await web3.eth.getBlockNumber()) + 1
    );

    expect((await aave.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await weth.balanceOf(arbrito.address)).toString()).equal("0");

    expect((await aave.balanceOf(owner)).toString()).equal(
      web3.utils.toWei("0.092600543527636767", "ether")
    );
    expect((await weth.balanceOf(owner)).toString()).equal("0");
  });
});
