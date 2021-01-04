use crate::pairs::Token;
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};
use web3::types::{Transaction, H160, H256, U256};

#[derive(Debug)]
pub enum Swap {
    UniswapSwap(UniswapSwap),
    BalancerSwap(BalancerSwap),
}

impl Swap {
    pub fn from_transaction(
        tx: &Transaction,
        uniswap_router_address: H160,
        balancer_pools: &HashSet<H160>,
        tokens: &HashMap<H160, Token>,
    ) -> Option<Swap> {
        if let Some(s) = UniswapSwap::from_transaction(tx, uniswap_router_address, tokens) {
            return Some(Swap::UniswapSwap(s));
        }

        if let Some(s) = BalancerSwap::from_transaction(tx, balancer_pools, tokens) {
            return Some(Swap::BalancerSwap(s));
        }

        None
    }

    pub fn conflicts(&self, token_from: H160, token_to: H160, balancer_pool: H160) -> bool {
        match self {
            Swap::UniswapSwap(s) => s.conflicts(token_from, token_to),
            Swap::BalancerSwap(s) => s.conflicts(token_from, token_to, balancer_pool),
        }
    }

    pub fn gas_price(&self) -> U256 {
        match self {
            Swap::UniswapSwap(s) => s.gas_price,
            Swap::BalancerSwap(s) => s.gas_price,
        }
    }

    pub fn tx_hash(&self) -> H256 {
        match self {
            Swap::UniswapSwap(s) => s.tx_hash,
            Swap::BalancerSwap(s) => s.tx_hash,
        }
    }
}

#[derive(Debug)]
enum UniswapSwapMethod {
    ExactTokensForTokens,
    ExactETHForTokens,
    ExactTokensForETH,
    TokensForExactTokens,
    TokensForExactETH,
    ETHForExactTokens,
}

pub struct UniswapSwap {
    method: UniswapSwapMethod,
    tokens: Vec<Option<Token>>,
    gas_price: U256,
    tx_hash: H256,
}

impl Debug for UniswapSwap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}({})",
            self.method,
            self.tokens
                .iter()
                .map(|token| token.as_ref().map_or("?", |token| &token.symbol))
                .collect::<Vec<_>>()
                .join(" -> ")
        )
    }
}

impl UniswapSwap {
    fn from_transaction(
        tx: &Transaction,
        uniswap_router_address: H160,
        tokens: &HashMap<H160, Token>,
    ) -> Option<UniswapSwap> {
        tx.to.filter(|&to| to == uniswap_router_address)?;

        if tx.input.0.len() < 4 {
            return None;
        }

        if (tx.input.0.len() - 4) % 32 != 0 {
            log::warn!("unsupported input size");
            return None;
        }

        let (method, tokens_offset) = match &tx.input.0[0..4] {
            [0x88, 0x03, 0xdb, 0xee] => (UniswapSwapMethod::TokensForExactTokens, 7),
            [0x38, 0xed, 0x17, 0x39] => (UniswapSwapMethod::ExactTokensForTokens, 7),
            [0x4a, 0x25, 0xd9, 0x4a] => (UniswapSwapMethod::TokensForExactETH, 7),
            [0x18, 0xcb, 0xaf, 0xe5] => (UniswapSwapMethod::ExactTokensForETH, 7),
            [0xfb, 0x3b, 0xdb, 0x41] => (UniswapSwapMethod::ETHForExactTokens, 6),
            [0x7f, 0xf3, 0x6a, 0xb5] => (UniswapSwapMethod::ExactETHForTokens, 6),
            _ => return None,
        };

        let mut addr: H160 = H160::zero();
        let token_matches: Vec<_> = tx.input.0[4..]
            .chunks_exact(32)
            .skip(tokens_offset - 1)
            .map(|chunk| {
                if chunk[0..12].iter().any(|b| *b != 0) {
                    return None;
                }

                addr.assign_from_slice(&chunk[12..32]);
                tokens.get(&addr).cloned()
            })
            .collect();

        if token_matches.iter().all(Option::is_none) {
            return None;
        }

        Some(UniswapSwap {
            tokens: token_matches,
            gas_price: tx.gas_price,
            tx_hash: tx.hash,
            method,
        })
    }

    fn conflicts(&self, token_from: H160, token_to: H160) -> bool {
        self.tokens.iter().tuple_windows().any(|swap| match swap {
            (Some(from), Some(to)) => from.address == token_from && to.address == token_to,
            _ => false,
        })
    }
}

#[derive(Debug)]
enum BalancerSwapMethod {
    ExactAmountOut,
    ExactAmountIn,
}

pub struct BalancerSwap {
    method: BalancerSwapMethod,
    token_in: Option<Token>,
    token_out: Option<Token>,
    gas_price: U256,
    tx_hash: H256,
    pool: H160,
}

impl BalancerSwap {
    fn from_transaction(
        tx: &Transaction,
        balancer_pools: &HashSet<H160>,
        tokens: &HashMap<H160, Token>,
    ) -> Option<BalancerSwap> {
        let pool = tx.to.filter(|to| balancer_pools.contains(to))?;

        if (tx.input.0.len() - 4) % 32 != 0 {
            log::warn!("unsupported input size");
            return None;
        }

        let method = match &tx.input.0[0..4] {
            [0x82, 0x01, 0xaa, 0x3f] => BalancerSwapMethod::ExactAmountIn,
            [0x7c, 0x5e, 0x9e, 0xa4] => BalancerSwapMethod::ExactAmountOut,
            _ => return None,
        };

        if tx.input.0[4..16].iter().any(|b| *b != 0) || tx.input.0[68..80].iter().any(|b| *b != 0) {
            return None;
        }

        let token_in = H160::from_slice(&tx.input.0[16..36]);
        let token_in = tokens.get(&token_in).cloned();

        let token_out = H160::from_slice(&tx.input.0[80..100]);
        let token_out = tokens.get(&token_out).cloned();

        if let (None, None) = (&token_in, &token_out) {
            return None;
        }

        Some(BalancerSwap {
            gas_price: tx.gas_price,
            tx_hash: tx.hash,
            token_out,
            token_in,
            method,
            pool,
        })
    }

    fn conflicts(&self, token_in: H160, token_out: H160, pool: H160) -> bool {
        if self.pool != pool {
            return false;
        }

        let ti_address = self.token_in.as_ref().map(|t| t.address);
        let in_is_in = ti_address.map_or(false, |addr| token_in == addr);

        let to_address = self.token_out.as_ref().map(|t| t.address);
        let out_is_out = to_address.map_or(false, |addr| token_out == addr);

        in_is_in || out_is_out
    }
}

impl Debug for BalancerSwap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}({})",
            self.method,
            vec![&self.token_in, &self.token_out]
                .iter()
                .map(|token| token.as_ref().map_or("?", |token| &token.symbol))
                .collect::<Vec<_>>()
                .join(" -> ")
        )
    }
}
