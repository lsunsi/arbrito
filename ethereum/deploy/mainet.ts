import { HardhatRuntimeEnvironment } from "hardhat/types";
import { DeployFunction } from "hardhat-deploy/types";

const deploy: DeployFunction = async ({
  deployments: { deploy },
  getNamedAccounts,
}: HardhatRuntimeEnvironment) => {
  const { deployer } = await getNamedAccounts();

  await deploy("Arbrito", {
    from: deployer,
    gasLimit: 600000,
    gasPrice: "12000000000",
    log: true,
    args: [],
  });
};

export default deploy;
