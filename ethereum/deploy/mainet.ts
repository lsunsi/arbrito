import { HardhatRuntimeEnvironment } from "hardhat/types";
import { DeployFunction } from "hardhat-deploy/types";

const deploy: DeployFunction = async ({
  deployments: { deploy },
  getNamedAccounts,
}: HardhatRuntimeEnvironment) => {
  const { deployer } = await getNamedAccounts();

  await deploy("Arbrito", {
    from: deployer,
    gasLimit: 1_000_000,
    gasPrice: "0x4a817c800", // 20gwei
    log: true,
    args: [],
  });
};

export default deploy;
