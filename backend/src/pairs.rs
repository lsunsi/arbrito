use serde::{Deserialize, Serialize};
use std::error::Error;
use web3::types::H160;

const FILE_PATH: &str = "pairs.toml";

#[derive(Serialize, Deserialize, Debug)]
pub struct Token {
    pub address: H160,
    pub symbol: String,
    pub decimals: usize,
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

impl Pair {
    pub fn new(token0: H160, token1: H160, balancer: H160, uniswap: H160) -> Option<Pair> {
        if token1 < token0 {
            Some(Pair {
                balancer,
                uniswap,
                token0: token1,
                token1: token0,
            })
        } else if token0 < token1 {
            Some(Pair {
                balancer,
                uniswap,
                token0,
                token1,
            })
        } else {
            None
        }
    }
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
