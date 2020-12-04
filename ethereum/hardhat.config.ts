import "@nomiclabs/hardhat-truffle5";
import "@nomiclabs/hardhat-web3";
import "@nomiclabs/hardhat-etherscan";
import "hardhat-gas-reporter";
import "hardhat-deploy";
import { HardhatUserConfig } from "hardhat/config";

const config: HardhatUserConfig = {
  solidity: {
    version: "0.7.5",
    settings: {
      optimizer: {
        enabled: true,
      },
    },
  },
  networks: {
    mainnet: {
      url: "http://127.0.0.1:8545",
      accounts: {
        mnemonic: process.env["ARBRITO_MNEMONIC"] || "",
        initialIndex: 0,
        count: 1,
      },
    },
    // hardhat: {
    //   forking: {
    //     url: "http://127.0.0.1:8545",
    //     blockNumber: 11388519,
    //   },
    // },
  },
  namedAccounts: {
    deployer: {
      1: 0,
    },
  },
  gasReporter: {
    currency: "BRL",
    gasPrice: 100,
    coinmarketcap: process.env["COINMARKETCAP_API_KEY"] || "",
  },
  etherscan: {
    apiKey: process.env["ETHERSCAN_API_KEY"],
  },
};

export default config;
