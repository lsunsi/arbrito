require("@nomiclabs/hardhat-truffle5");
require("@nomiclabs/hardhat-web3");
require("hardhat-gas-reporter");

module.exports = {
  solidity: {
    version: "0.7.4",
    settings: {
      optimizer: {
        enabled: true,
      },
    },
  },

  gasReporter: {
    currency: "BRL",
    gasPrice: 50,
    ethPrice: 2500,
  },

  // networks: {
  //   hardhat: {
  //     forking: {
  //       url: "http://127.0.0.1:8545",
  //       blockNumber: 11238167,
  //     },
  //   },
  // },
};
