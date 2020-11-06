require("@nomiclabs/hardhat-truffle5");
require("@nomiclabs/hardhat-web3");

module.exports = {
  solidity: {
    version: "0.7.2",
    settings: {
      optimizer: {
        enabled: true,
      },
    },
  },
};
