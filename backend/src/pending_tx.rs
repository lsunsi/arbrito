use crate::pairs::Token;
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    str::FromStr,
};
use web3::types::{Transaction, H160, H256, U256};

#[derive(Debug)]
pub struct PendingTx {
    pub gas_price: U256,
    pub hash: H256,
    kind: Kind,
}

impl PendingTx {
    pub fn from_transaction(
        tx: &Transaction,
        uniswap_router_address: H160,
        balancer_pools: &HashSet<H160>,
        tokens: &HashMap<H160, Token>,
    ) -> Option<PendingTx> {
        if tx.input.0.len() < 4 {
            return None;
        }

        WeskerOperation::parse_kind(tx)
            .or_else(|| BalancerSwap::parse_kind(tx, balancer_pools, tokens))
            .or_else(|| UniswapSwap::parse_kind(tx, uniswap_router_address, tokens))
            .map(|kind| PendingTx {
                gas_price: tx.gas_price,
                hash: tx.hash,
                kind,
            })
    }

    pub fn conflicts(&self, token_from: H160, token_to: H160, balancer_pool: H160) -> bool {
        match &self.kind {
            Kind::UniswapSwap(s) => s.conflicts(token_from, token_to),
            Kind::BalancerSwap(s) => s.conflicts(token_from, token_to, balancer_pool),
            Kind::WeskerOperation(_) => true,
        }
    }
}

#[derive(Debug)]
enum Kind {
    UniswapSwap(UniswapSwap),
    BalancerSwap(BalancerSwap),
    WeskerOperation(WeskerOperation),
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

struct UniswapSwap {
    method: UniswapSwapMethod,
    tokens: Vec<Option<Token>>,
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
    fn parse_kind(
        tx: &Transaction,
        uniswap_router_address: H160,
        tokens: &HashMap<H160, Token>,
    ) -> Option<Kind> {
        tx.to.filter(|&to| to == uniswap_router_address)?;

        if (tx.input.0.len() - 4) % 32 != 0 {
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

        Some(Kind::UniswapSwap(UniswapSwap {
            tokens: token_matches,
            method,
        }))
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

struct BalancerSwap {
    method: BalancerSwapMethod,
    token_in: Option<Token>,
    token_out: Option<Token>,
    pool: H160,
}

impl BalancerSwap {
    fn parse_kind(
        tx: &Transaction,
        balancer_pools: &HashSet<H160>,
        tokens: &HashMap<H160, Token>,
    ) -> Option<Kind> {
        let pool = tx.to.filter(|to| balancer_pools.contains(to))?;

        if (tx.input.0.len() - 4) % 32 != 0 {
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

        Some(Kind::BalancerSwap(BalancerSwap {
            token_out,
            token_in,
            method,
            pool,
        }))
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

#[derive(Debug)]
struct WeskerOperation;

impl WeskerOperation {
    fn parse_kind(tx: &Transaction) -> Option<Kind> {
        if tx.to != Some(H160::from_str("0000000000007f150bd6f54c40a34d7c3d5e9f56").unwrap()) {
            return None;
        }

        match &tx.input.0[0..4] {
            [0x03, 0x03, 0x19, 0x1c]
            | [0x00, 0x03, 0x19, 0x1c]
            | [0x03, 0x02, 0x19, 0x1c]
            | [0x01, 0x02, 0x19, 0x1c]
            | [0x01, 0x03, 0x19, 0x1c]
            | [0x03, 0x02, 0xe8, 0x92]
            | [0x00, 0x02, 0x19, 0x1c]
            | [0x01, 0x02, 0xe8, 0x92]
            | [0x00, 0x02, 0xe8, 0x92]
            | [0x03, 0x03, 0xe8, 0x92] => Some(Kind::WeskerOperation(WeskerOperation)),
            _ => None,
        }
    }
}
