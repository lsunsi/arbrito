const Arbrito = artifacts.require("Arbrito");
const Link = artifacts.require("Link");
const Weth = artifacts.require("Weth");
const Aave = artifacts.require("Aave");
const Uniswap = artifacts.require("Uniswap");
const Balancer = artifacts.require("Balancer");

const deployContracts = async () => {
  const ethAddress = "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE";
  const link = await Link.new();
  const weth = await Weth.new();

  const uniswap = await Uniswap.new(link.address, weth.address);
  const balancer = await Balancer.new(link.address, weth.address);
  const aave = await Aave.new(ethAddress);

  const arbrito = await Arbrito.new(
    ethAddress,
    weth.address,
    aave.address,
    uniswap.address
  );

  return [arbrito, link, weth, uniswap, balancer, aave];
};

contract("Arbrito", ([owner, other]) => {
  it("allows for setting of the wethInput and minWethProfit by the owner", async () => {
    const [arbrito] = await deployContracts();

    expect((await arbrito.wethInput()).toString()).equal(
      web3.utils.toWei("10", "ether")
    );
    expect((await arbrito.wethMinProfit()).toString()).equal(
      web3.utils.toWei("0.1", "ether")
    );

    await arbrito.setWethInput(1, 2, { from: owner });

    expect((await arbrito.wethInput()).toString()).equal("1");
    expect((await arbrito.wethMinProfit()).toString()).equal("2");

    let error;
    try {
      await arbrito.setWethInput(1, 2, { from: other });
    } catch (e) {
      error = e;
    }

    expect(error).match(/so no/);
  });

  it("allows for eth transfers from anyone into the contract", async () => {
    const [arbrito] = await deployContracts();

    const balanceBefore = await web3.eth.getBalance(arbrito.address);

    await web3.eth.sendTransaction({
      from: other,
      to: arbrito.address,
      value: 123,
    });

    const balanceAfter = await web3.eth.getBalance(arbrito.address);

    expect(balanceAfter - balanceBefore).equal(123);
  });

  it("refuses trying anything if uniswap and balancer mostly agree on price", async () => {
    const [arbrito, link, weth, uniswap, balancer] = await deployContracts();

    // Uniswap's LINK price is 0.1 ETH
    await link.mint(uniswap.address, 10000);
    await weth.mint(uniswap.address, 1000);
    // Balancer's LINK price is 0.0999 ETH
    await link.mint(balancer.address, 10000);
    await weth.mint(balancer.address, 999);

    let error = null;
    try {
      await arbrito.perform(link.address, balancer.address);
    } catch (e) {
      error = e;
    }

    // Nah
    expect(error).match(/Not worth our trouble/);

    // Balancer's LINK price is 0.1 ETH
    await weth.mint(balancer.address, 1);

    error = null;
    try {
      await arbrito.perform(link.address, balancer.address);
    } catch (e) {
      error = e;
    }

    // Still nah
    expect(error).match(/The pools agree on the price/);

    // Balancer's LINK price is 0.1001 ETH
    await weth.mint(balancer.address, 1);

    error = null;
    try {
      await arbrito.perform(link.address, balancer.address);
    } catch (e) {
      error = e;
    }

    // Still still nah
    expect(error).match(/Not worth our trouble/);
  });

  it("extracts profit if the token's cheaper on balancer than uniswap", async () => {
    const [
      arbrito,
      link,
      weth,
      uniswap,
      balancer,
      aave,
    ] = await deployContracts();

    // Uniswap's LINK price is 0.1 ETH
    await link.mint(uniswap.address, web3.utils.toWei("1000", "ether"));
    await weth.mint(uniswap.address, web3.utils.toWei("100", "ether"));
    // Balancer's LINK price is 0.09 ETH
    await link.mint(balancer.address, web3.utils.toWei("10000", "ether"));
    await weth.mint(balancer.address, web3.utils.toWei("900", "ether"));

    // Weth needs some ETH to give back as profit
    await web3.eth.sendTransaction({
      value: web3.utils.toWei("2", "ether"),
      to: weth.address,
      from: other,
    });

    // Aaave needs to have some ETH to lend
    await web3.eth.sendTransaction({
      value: web3.utils.toWei("10", "ether"),
      to: aave.address,
      from: other,
    });

    // Try to arbitrage
    const ownerBalanceBefore = new web3.utils.BN(
      await web3.eth.getBalance(owner)
    );
    const tx = await arbrito.perform(link.address, balancer.address);

    // Uniswap's LINK price is not 0.08 ETH
    expect((await link.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("1111.111111111111111111", "ether")
    );
    expect((await weth.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("88.888888888888888889", "ether")
    );

    // Balancer's LINK price is not 0.092022472 ETH
    expect((await link.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("9888.888888888888888889", "ether")
    );
    expect((await weth.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("910", "ether")
    );

    // Contract keeps nothing
    expect((await link.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await weth.balanceOf(arbrito.address)).toString()).equal("0");
    expect(await web3.eth.getBalance(arbrito.address)).equal("0");

    // Verify arbitrage worked
    const ownerBalanceAfter = new web3.utils.BN(
      await web3.eth.getBalance(owner)
    );

    const gasCost = new web3.utils.BN(await web3.eth.getGasPrice()).mul(
      new web3.utils.BN(tx.receipt.gasUsed)
    );

    expect(
      ownerBalanceAfter.add(gasCost).sub(ownerBalanceBefore).toString()
    ).equal(web3.utils.toWei("1.011111111111111111", "ether"));
  });

  it("extracts profit if the token's cheaper on uniswap than balancer", async () => {
    const [
      arbrito,
      link,
      weth,
      uniswap,
      balancer,
      aave,
    ] = await deployContracts();

    // Uniswap's LINK price is 0.09 ETH
    await link.mint(uniswap.address, web3.utils.toWei("10000", "ether"));
    await weth.mint(uniswap.address, web3.utils.toWei("900", "ether"));
    // Balancer's LINK price is 0.1 ETH
    await link.mint(balancer.address, web3.utils.toWei("1000", "ether"));
    await weth.mint(balancer.address, web3.utils.toWei("100", "ether"));

    // Weth needs some ETH to give back as profit
    await web3.eth.sendTransaction({
      value: web3.utils.toWei("2", "ether"),
      to: weth.address,
      from: other,
    });

    // Aaave needs to have some ETH to lend
    await web3.eth.sendTransaction({
      value: web3.utils.toWei("10", "ether"),
      to: aave.address,
      from: other,
    });

    // Try to arbitrage
    const ownerBalanceBefore = new web3.utils.BN(
      await web3.eth.getBalance(owner)
    );
    const tx = await arbrito.perform(link.address, balancer.address);

    // Uniswap's LINK price is not 0.092022472 ETH
    expect((await link.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("9888.888888888888888889", "ether")
    );
    expect((await weth.balanceOf(uniswap.address)).toString()).equal(
      web3.utils.toWei("910", "ether")
    );

    // Balancer's LINK price is not 0.08 ETH
    expect((await link.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("1111.111111111111111111", "ether")
    );
    expect((await weth.balanceOf(balancer.address)).toString()).equal(
      web3.utils.toWei("88.888888888888888889", "ether")
    );

    // Contract keeps nothing
    expect((await link.balanceOf(arbrito.address)).toString()).equal("0");
    expect((await weth.balanceOf(arbrito.address)).toString()).equal("0");
    expect(await web3.eth.getBalance(arbrito.address)).equal("0");

    // Verify arbitrage worked
    const ownerBalanceAfter = new web3.utils.BN(
      await web3.eth.getBalance(owner)
    );

    const gasCost = new web3.utils.BN(await web3.eth.getGasPrice()).mul(
      new web3.utils.BN(tx.receipt.gasUsed)
    );

    expect(
      ownerBalanceAfter.add(gasCost).sub(ownerBalanceBefore).toString()
    ).equal(web3.utils.toWei("1.011111111111111111", "ether"));
  });
});
