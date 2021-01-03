use crate::pairs::Token;
use itertools::Itertools;
use std::{collections::HashMap, fmt::Debug};
use web3::types::{Transaction, H160, H256, U256};

#[derive(Debug)]
pub enum UniswapSwapMethod {
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
    pub gas_price: U256,
    pub tx_hash: H256,
}

pub enum UniswapSwapMatch {
    OppositeDirection,
    SameDirection,
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
    pub fn from_transaction(
        tx: &Transaction,
        tokens: &HashMap<H160, Token>,
    ) -> Option<UniswapSwap> {
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

    pub fn tokens_match(&self, token_from: H160, token_to: H160) -> Option<UniswapSwapMatch> {
        for (from, to) in self.tokens.iter().tuple_windows() {
            let (from, to) = match (from, to) {
                (Some(from), Some(to)) => (from, to),
                _ => continue,
            };

            if from.address == token_from && to.address == token_to {
                return Some(UniswapSwapMatch::SameDirection);
            }

            if from.address == token_to && to.address == token_from {
                return Some(UniswapSwapMatch::OppositeDirection);
            }
        }

        None
    }
}
