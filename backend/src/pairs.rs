use serde::{Deserialize, Serialize};
use std::error::Error;
use web3::types::H160;

const FILE_PATH: &str = "pairs.toml";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Token {
    pub address: H160,
    pub symbol: String,
    pub decimals: usize,
    pub weth_uniswap_pair: Option<H160>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pair {
    pub balancer: H160,
    pub uniswap: H160,
    pub token0: H160,
    pub token1: H160,
}

#[derive(Serialize, Deserialize)]
pub struct Pairs {
    pub tokens: Vec<Token>,
    pub pairs: Vec<Pair>,
}

impl Pairs {
    pub fn read() -> Result<Self, Box<dyn Error>> {
        let bytes = std::fs::read(FILE_PATH)?;
        let pairs = toml::from_slice(&bytes)?;
        Ok(pairs)
    }

    pub fn write(self) -> Result<(), Box<dyn Error>> {
        let string = toml::to_string(&self)?;
        std::fs::write(FILE_PATH, string)?;
        Ok(())
    }
}
