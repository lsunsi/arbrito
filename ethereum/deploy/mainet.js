module.exports = async ({ deployments: { deploy }, getNamedAccounts }) => {
  const { deployer } = await getNamedAccounts();

  await deploy("Arbrito", {
    from: deployer,
    gasLimit: 600000,
    gasPrice: 12000000000,
    log: true,
    args: [],
  });
};
