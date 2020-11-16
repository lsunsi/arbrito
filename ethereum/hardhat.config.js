require("@nomiclabs/hardhat-truffle5");
require("@nomiclabs/hardhat-web3");
require("hardhat-gas-reporter");
require("hardhat-deploy");

module.exports = {
  solidity: {
    version: "0.7.4",
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
        mnemonic: process.env["ARBRITO_MNEMONIC"] || '',
        initialIndex: 0,
        count: 1,
      },
    },
    //   hardhat: {
    //     forking: {
    //       url: "http://127.0.0.1:8545",
    //       blockNumber: 11238167,
    //     },
    //   },
  },
  namedAccounts: {
    deployer: {
      1: 0,
    },
  },
  gasReporter: {
    currency: "BRL",
    gasPrice: 50,
    ethPrice: 2500,
  },
};
