import { HardhatRuntimeEnvironment } from "hardhat/types";
import { DeployFunction } from "hardhat-deploy/types";

const deploy: DeployFunction = async ({
  deployments: { deploy },
  getNamedAccounts,
}: HardhatRuntimeEnvironment) => {
  const { deployer } = await getNamedAccounts();

  await deploy("Arbrito", {
    from: deployer,
    gasLimit: 1_500_000,
    gasPrice: "0x37e11d600", // 15gwei
    log: true,
    args: [
      "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
      "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
    ],
  });
};

export default deploy;
