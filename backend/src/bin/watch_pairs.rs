use colored::Colorize;
use ethcontract::{Account, BlockNumber, GasPrice, Password, TransactionCondition};
use futures::FutureExt;
use pooller::{
    gen::{Arbrito, Balancer, UniswapPair},
    max_profit, uniswap_out_given_in, Pairs,
};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::{mpsc::unbounded_channel, Mutex, OwnedMutexGuard};
use web3::{
    futures::{future::join_all, StreamExt},
    transports::WebSocket,
    types::U64,
    types::{H160, U256},
    Web3,
};

const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const ARBRITO_ADDRESS: &str = "e96B6680a8ef1D8d561171948226bc3A133fA56D";
const EXECUTOR_ADDRESS: &str = "Af43007aD675D6C72E96905cf4d8acB58ba0E041";
const WEB3_ENDPOINT: &str = "ws://127.0.0.1:8546";
const TARGET_NET_PROFIT: u128 = 10_000_000_000_000_000; // 0.01 eth
const EXPECTED_GAS_USAGE: u128 = 350_000;
const MAX_GAS_USAGE: u128 = 400_000;
const MIN_GAS_SCALE: u8 = 2;

fn format_amount_colored(token: &ArbritageToken, amount: U256) -> String {
    let string = format_amount(token, amount);

    if amount >= U256::exp10(18) {
        string.bright_green().bold().italic().underline()
    } else if amount >= U256::exp10(17) {
        string.bright_green().bold()
    } else if amount >= U256::exp10(16) {
        string.green()
    } else if amount >= U256::exp10(15) {
        string.yellow().dimmed()
    } else {
        string.dimmed()
    }
    .to_string()
}

fn format_amount(token: &ArbritageToken, amount: U256) -> String {
    let decimals = U256::exp10(token.decimals);
    format!(
        "{} {}.{:03$}",
        token.symbol,
        (amount / decimals).as_u128(),
        (amount % decimals).as_u128(),
        token.decimals,
    )
}

fn format_block_number(number: U64) -> String {
    format!(
        "{}{}",
        if number.as_u64() % 2 == 0 {
            "#".bright_magenta()
        } else {
            "#".bright_cyan()
        },
        number.to_string().bright_white().dimmed()
    )
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
enum ArbritageResult {
    NotProfit,
    GrossProfit {
        weth_profit: U256,
        amount: U256,
    },
    NetProfit {
        weth_profit: U256,
        gas_price: U256,
        amount: U256,
    },
}

#[derive(Debug, Clone)]
struct ArbritageToken {
    weth_uniswap_pair: Option<UniswapPair>,
    decimals: usize,
    symbol: String,
    address: H160,
}

#[derive(Debug, Clone)]
struct ArbritageAttempt {
    pair: ArbritagePair,
    tokens: (ArbritageToken, ArbritageToken),
    result: ArbritageResult,
    max_gas_usage: U256,
    block_number: U64,
}

#[derive(Clone, Debug)]
struct ArbritagePair {
    uniswap_pair: UniswapPair,
    balancer: Balancer,
    token0: ArbritageToken,
    token1: ArbritageToken,
    weth: ArbritageToken,
}

impl ArbritagePair {
    async fn run(
        &self,
        borrow_token: &ArbritageToken,
        profit_token: &ArbritageToken,
        min_gas_price: U256,
        expected_gas_usage: U256,
        target_net_profit: U256,
    ) -> ArbritageResult {
        let (reserve0, reserve1, _) = self
            .uniswap_pair
            .get_reserves()
            .call()
            .await
            .expect("uniswap_pair get_reserves failed");

        let (ro, ri) = if self.token0.address == borrow_token.address {
            (reserve0, reserve1)
        } else {
            (reserve1, reserve0)
        };

        let bi = self
            .balancer
            .get_balance(borrow_token.address)
            .call()
            .await
            .expect("balancer get_balance(source) failed");
        let bo = self
            .balancer
            .get_balance(profit_token.address)
            .call()
            .await
            .expect("balancer get_balance(target) failed");
        let s = self
            .balancer
            .get_swap_fee()
            .call()
            .await
            .expect("balancer get_swap_fee failed");

        let (amount, profit) = match max_profit(U256::from(ri), U256::from(ro), bi, bo, s) {
            None => return ArbritageResult::NotProfit,
            Some(a) => a,
        };

        let weth_profit = if profit_token.address == self.weth.address {
            profit
        } else {
            let profit_pair = profit_token
                .weth_uniswap_pair
                .as_ref()
                .expect("required uniswap pair missing");

            let (reserve0, reserve1, _) = profit_pair
                .get_reserves()
                .call()
                .await
                .expect("uniswap_pair profit get_reserves failed");

            let token0address = profit_pair
                .token_0()
                .call()
                .await
                .expect("uniswap_pair profit token0 failed");

            let (ro, ri) = if token0address == profit_token.address {
                (reserve1, reserve0)
            } else {
                (reserve0, reserve1)
            };

            uniswap_out_given_in(U256::from(ri), U256::from(ro), profit)
        };

        if weth_profit <= target_net_profit {
            return ArbritageResult::GrossProfit {
                weth_profit,
                amount,
            };
        }

        let gas_price = (weth_profit - target_net_profit) / expected_gas_usage;

        if gas_price < min_gas_price {
            ArbritageResult::GrossProfit {
                weth_profit,
                amount,
            }
        } else {
            ArbritageResult::NetProfit {
                weth_profit,
                gas_price,
                amount,
            }
        }
    }

    async fn attempt(
        &self,
        borrow_token: ArbritageToken,
        profit_token: ArbritageToken,
        min_gas_price: U256,
        expected_gas_usage: U256,
        max_gas_usage: U256,
        target_net_profit: U256,
        block_number: U64,
    ) -> ArbritageAttempt {
        let result = self
            .run(
                &borrow_token,
                &profit_token,
                min_gas_price,
                expected_gas_usage,
                target_net_profit,
            )
            .await;

        ArbritageAttempt {
            pair: self.clone(),
            tokens: (borrow_token, profit_token),
            max_gas_usage,
            block_number,
            result,
        }
    }

    async fn attempts(
        &self,
        min_gas_price: U256,
        expected_gas_usage: U256,
        max_gas_usage: U256,
        target_net_profit: U256,
        block_number: U64,
    ) -> Vec<ArbritageAttempt> {
        vec![
            self.attempt(
                self.token0.clone(),
                self.token1.clone(),
                min_gas_price,
                expected_gas_usage,
                max_gas_usage,
                target_net_profit,
                block_number,
            )
            .await,
            self.attempt(
                self.token1.clone(),
                self.token0.clone(),
                min_gas_price,
                expected_gas_usage,
                max_gas_usage,
                target_net_profit,
                block_number,
            )
            .await,
        ]
    }
}

async fn execute(
    _: OwnedMutexGuard<()>,
    attempt: ArbritageAttempt,
    arbrito: Arbrito,
    from_address: H160,
    web3: Web3<WebSocket>,
) {
    let (amount, gas_price) = match attempt.result {
        ArbritageResult::NetProfit {
            weth_profit,
            gas_price,
            amount,
        } => {
            log::info!(
                "{} {}: borrow {} for {} profit ({}) @ {} gwei",
                format_block_number(attempt.block_number),
                "Executing attempt".bold().underline(),
                format_amount(&attempt.tokens.0, amount),
                attempt.tokens.1.symbol,
                format_amount_colored(&attempt.pair.weth, weth_profit),
                gas_price / U256::exp10(9)
            );
            log::debug!(
                "Token addresses = {} {}",
                attempt.tokens.0.address,
                attempt.tokens.1.address
            );
            log::debug!("UniswapPool = {}", attempt.pair.uniswap_pair.address());
            log::debug!("BalancerPool = {}", attempt.pair.balancer.address());

            (amount, gas_price)
        }
        _ => {
            log::error!(
                "{} Cannot execute non-net-profitable attempt",
                format_block_number(attempt.block_number)
            );
            return;
        }
    };

    let nonce = web3
        .eth()
        .transaction_count(
            from_address,
            Some(BlockNumber::Number(attempt.block_number)),
        )
        .await
        .expect("failed fetching nonce");

    let borrow = if attempt.pair.token0.address == attempt.tokens.0.address {
        0
    } else {
        1
    };

    let (reserve0, reserve1, _) = attempt
        .pair
        .uniswap_pair
        .get_reserves()
        .call()
        .await
        .expect("failed getting reserves");

    let tx = arbrito
        .perform(
            borrow,
            amount,
            attempt.pair.uniswap_pair.address(),
            attempt.pair.balancer.address(),
            attempt.pair.token0.address,
            attempt.pair.token1.address,
            U256::from(reserve0),
            U256::from(reserve1),
            U256::from(attempt.block_number.as_u64() + 1),
        )
        .from(Account::Locked(
            from_address,
            Password::new(std::env::var("ARBRITO_EXEC_PASSWORD").unwrap()),
            Some(TransactionCondition::Block(attempt.block_number.as_u64())),
        ))
        .gas(attempt.max_gas_usage)
        .gas_price(GasPrice::Value(gas_price))
        .confirmations(1)
        .nonce(nonce)
        .send()
        .await;

    match tx {
        Err(_) => log::info!(
            "{} {}",
            format_block_number(attempt.block_number),
            "Arbitrage execution failed".red().dimmed(),
        ),
        Ok(tx) => log::info!(
            "{} {} Transaction hash {}",
            format_block_number(attempt.block_number),
            "Arbitrage execution succeeded!".bright_green().bold(),
            tx.hash()
        ),
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let web3 = Web3::new(WebSocket::new(WEB3_ENDPOINT).await.expect("ws failed"));

    let weth_address = H160::from_str(WETH_ADDRESS).expect("failed parsing weth address");

    let Pairs { tokens, pairs } = Pairs::read().expect("pairs reading failed");
    let tokens: HashMap<_, _> = tokens
        .into_iter()
        .map(|t| {
            (
                t.address,
                ArbritageToken {
                    weth_uniswap_pair: t.weth_uniswap_pair.map(|a| UniswapPair::at(&web3, a)),
                    decimals: t.decimals,
                    address: t.address,
                    symbol: t.symbol,
                },
            )
        })
        .collect();

    let weth = tokens.get(&weth_address).expect("where's my weth, boy?");

    let arbrito_address = H160::from_str(ARBRITO_ADDRESS).expect("failed parsing arbrito address");
    let arbrito = Arbrito::at(&web3, arbrito_address);

    let executor_address =
        H160::from_str(EXECUTOR_ADDRESS).expect("failed parsing executor address");

    let target_net_profit = U256::from(TARGET_NET_PROFIT);
    let expected_gas_usage = U256::from(EXPECTED_GAS_USAGE);
    let max_gas_usage = U256::from(MAX_GAS_USAGE);
    let (tx, mut rx) = unbounded_channel::<ArbritageAttempt>();

    let executor_web3 = web3.clone();
    tokio::spawn(async move {
        let lock = Arc::new(Mutex::new(()));
        let mut block_number = U64::from(0);

        while let Some(attempt) = rx.recv().await {
            if attempt.block_number <= block_number {
                log::warn!(
                    "{} Dropped profitable attempt",
                    format_block_number(block_number)
                );
                continue;
            }

            block_number = attempt.block_number;

            match lock.clone().try_lock_owned() {
                Err(_) => log::info!(
                    "{} Dropped profitable attempt waiting for previous one",
                    format_block_number(block_number)
                ),
                Ok(guard) => {
                    tokio::spawn(execute(
                        guard,
                        attempt,
                        arbrito.clone(),
                        executor_address,
                        executor_web3.clone(),
                    ));
                }
            };
        }
    });

    let arbritage_pairs: Vec<_> = pairs
        .into_iter()
        .map(|pair| ArbritagePair {
            token0: tokens.get(&pair.token0).expect("unknown token").clone(),
            token1: tokens.get(&pair.token1).expect("unknown token").clone(),
            balancer: Balancer::at(&web3, pair.balancer),
            uniswap_pair: UniswapPair::at(&web3, pair.uniswap),
            weth: weth.clone(),
        })
        .collect();

    web3.eth_subscribe()
        .subscribe_new_heads()
        .await
        .expect("failed subscribing to new heads")
        .for_each(|head| async {
            let block_number = match head.ok().and_then(|h| h.number) {
                Some(number) => number,
                None => return (),
            };

            log::info!("{} New block header", format_block_number(block_number));

            let min_gas_price = web3
                .eth()
                .gas_price()
                .await
                .expect("failed getting gas price")
                * MIN_GAS_SCALE;

            let min_required_profit = target_net_profit + min_gas_price * expected_gas_usage;

            log::info!(
                "{} Min required profit {} @ {} gwei",
                format_block_number(block_number),
                format_amount(&weth, min_required_profit),
                min_gas_price / U256::exp10(9)
            );

            let t = std::time::Instant::now();

            let attempt_futs = arbritage_pairs.iter().map(|pair| {
                pair.attempts(
                    min_gas_price,
                    expected_gas_usage,
                    max_gas_usage,
                    target_net_profit,
                    block_number,
                )
                .map(|attempts| {
                    for attempt in &attempts {
                        if let ArbritageResult::NetProfit { .. } = attempt.result {
                            tx.send(attempt.clone()).expect("where's the executor at?");
                        }
                    }
                    attempts
                })
            });

            let attempt = join_all(attempt_futs)
                .await
                .into_iter()
                .flatten()
                .max_by(|a1, a2| a1.result.cmp(&a2.result))
                .expect("empty arbritage results");

            match attempt.result {
                ArbritageResult::NotProfit => {
                    log::info!("{} No profit found", format_block_number(block_number))
                }
                ArbritageResult::GrossProfit {
                    weth_profit,
                    amount,
                }
                | ArbritageResult::NetProfit {
                    weth_profit,
                    amount,
                    ..
                } => {
                    log::info!(
                        "{} Highest profit found: borrow {} for {} profit ({})",
                        format_block_number(block_number),
                        format_amount(&attempt.tokens.0, amount),
                        attempt.tokens.1.symbol,
                        format_amount_colored(&weth, weth_profit),
                    );
                }
            }

            log::info!(
                "{} Processed in {:.2} seconds ({} pairs)",
                format_block_number(block_number),
                t.elapsed().as_secs_f64(),
                arbritage_pairs.len()
            );

            ()
        })
        .await;
}
