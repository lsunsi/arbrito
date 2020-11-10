const { expect } = require("hardhat");

const Arbrito = artifacts.require("Arbrito");
const ERC20Mintable = artifacts.require("ERC20Mintable");
const Uniswap = artifacts.require("Uniswap");
const Balancer = artifacts.require("Balancer");

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
    const [
      arbrito,
      uniswap,
      balancer,
      token0,
      token1,
    ] = await deployContracts();

    await token0.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await token1.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, web3.utils.toWei("10", "ether"));
    await token1.mint(balancer.address, web3.utils.toWei("30", "ether"));

    await arbrito.perform(
      true,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address
    );

    expect((await token0.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await token1.balanceOf(uniswap.address)).toString()).equal(
      "11114454474534715257"
    );

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
    const [
      arbrito,
      uniswap,
      balancer,
      token0,
      token1,
    ] = await deployContracts();

    await token0.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await token1.mint(uniswap.address, web3.utils.toWei("10", "ether"));
    await uniswap.refreshReserves();

    await token0.mint(balancer.address, web3.utils.toWei("30", "ether"));
    await token1.mint(balancer.address, web3.utils.toWei("10", "ether"));

    await arbrito.perform(
      false,
      web3.utils.toWei("1", "ether"),
      uniswap.address,
      balancer.address
    );

    expect((await token1.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("9", "ether")
    );
    expect((await token0.balanceOf(uniswap.address)).toString()).equal(
      "11114454474534715257"
    );

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
    const [
      arbrito,
      uniswap,
      balancer,
      token0,
      token1,
    ] = await deployContracts();

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
        balancer.address
      );
    } catch (e) {
      error = e;
    }

    expect(error).match(/Insufficient amount out/);
  });
});
