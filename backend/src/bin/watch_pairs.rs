use colored::Colorize;
use ethcontract::{Account, BlockId, BlockNumber, GasPrice, Password, TransactionCondition};
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

const WEB3_ENDPOINT: &str = "ws://127.0.0.1:8546";
const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const ARBRITO_ADDRESS: &str = "0aF72DF780386476558BD8E8EEB5c821209bfE95";
const EXECUTOR_ADDRESS: &str = "Af43007aD675D6C72E96905cf4d8acB58ba0E041";
const TARGET_WETH_PROFIT: u128 = 10_000_000_000_000_000; // 0.01 eth
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

#[derive(Debug, Clone, Copy)]
struct Block {
    number: U64,
    gas_price: U256,
    balance: U256,
    nonce: U256,
}

#[derive(Debug, Clone, Copy)]
struct Config {
    expected_gas_usage: U256,
    max_gas_usage: U256,
    target_weth_profit: U256,
    min_gas_scale: u8,
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
    config: Config,
    block: Block,
}

#[derive(Debug, Clone)]
struct ArbritagePair {
    uniswap_pair: UniswapPair,
    balancer: Balancer,
    token0: ArbritageToken,
    token1: ArbritageToken,
    weth: ArbritageToken,
}

impl Block {
    async fn fetch(web3: &Web3<WebSocket>, number: U64, addr: H160) -> Block {
        let eth = web3.eth();
        let (nonce, balance, gas_price) = tokio::join!(
            eth.transaction_count(addr, Some(BlockNumber::Number(number))),
            eth.balance(addr, Some(BlockNumber::Number(number))),
            eth.gas_price(),
        );

        Block {
            gas_price: gas_price.expect("failed fetching gas_price"),
            balance: balance.expect("failed fetching balance"),
            nonce: nonce.expect("failed fetching nonce"),
            number,
        }
    }
}

impl ArbritagePair {
    async fn run(
        &self,
        borrow_token: &ArbritageToken,
        profit_token: &ArbritageToken,
        config: Config,
        block: Block,
    ) -> ArbritageResult {
        let block_id = BlockId::Number(BlockNumber::Number(block.number));

        let (reserve0, reserve1, _) = self
            .uniswap_pair
            .get_reserves()
            .block(block_id)
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
            .block(block_id)
            .call()
            .await
            .expect("balancer get_balance(source) failed");
        let bo = self
            .balancer
            .get_balance(profit_token.address)
            .block(block_id)
            .call()
            .await
            .expect("balancer get_balance(target) failed");
        let s = self
            .balancer
            .get_swap_fee()
            .block(block_id)
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
                .block(block_id)
                .call()
                .await
                .expect("uniswap_pair profit get_reserves failed");

            let token0address = profit_pair
                .token_0()
                .block(block_id)
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

        if weth_profit <= config.target_weth_profit {
            return ArbritageResult::GrossProfit {
                weth_profit,
                amount,
            };
        }

        let max_gas_price = block.balance / config.max_gas_usage;
        let min_gas_price = block.gas_price * config.min_gas_scale;

        let target_gas_price =
            (weth_profit - config.target_weth_profit) / config.expected_gas_usage;

        if target_gas_price < min_gas_price {
            ArbritageResult::GrossProfit {
                weth_profit,
                amount,
            }
        } else {
            ArbritageResult::NetProfit {
                gas_price: target_gas_price.min(max_gas_price),
                weth_profit,
                amount,
            }
        }
    }

    async fn attempts(&self, config: Config, block: Block) -> Vec<ArbritageAttempt> {
        let max_gas_price = block.balance / config.max_gas_usage;
        let min_gas_price = block.gas_price * config.min_gas_scale;

        if max_gas_price < min_gas_price {
            log::warn!("max_gas_price < min_gas_price. attempts won't be calculated");
            return vec![];
        }

        vec![
            ArbritageAttempt {
                pair: self.clone(),
                result: self.run(&self.token0, &self.token1, config, block).await,
                tokens: (self.token0.clone(), self.token1.clone()),
                config,
                block,
            },
            ArbritageAttempt {
                pair: self.clone(),
                result: self.run(&self.token1, &self.token0, config, block).await,
                tokens: (self.token1.clone(), self.token0.clone()),
                config,
                block,
            },
        ]
    }
}

async fn execute(
    _: OwnedMutexGuard<()>,
    attempt: ArbritageAttempt,
    arbrito: Arbrito,
    from_address: H160,
) {
    let (amount, gas_price) = match attempt.result {
        ArbritageResult::NetProfit {
            weth_profit,
            gas_price,
            amount,
        } => {
            log::info!(
                "{} {}: borrow {} for {} profit ({}) @ {} gwei",
                format_block_number(attempt.block.number),
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
                format_block_number(attempt.block.number)
            );
            return;
        }
    };

    let borrow = if attempt.pair.token0.address == attempt.tokens.0.address {
        0
    } else {
        1
    };

    let (reserve0, reserve1, _) = attempt
        .pair
        .uniswap_pair
        .get_reserves()
        .block(BlockId::Number(BlockNumber::Number(attempt.block.number)))
        .call()
        .await
        .expect("failed getting reserves");

    let balance0 = attempt
        .pair
        .balancer
        .get_balance(attempt.pair.token0.address)
        .block(BlockId::Number(BlockNumber::Number(attempt.block.number)))
        .call()
        .await
        .expect("failed getting balances");

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
            balance0,
        )
        .from(Account::Locked(
            from_address,
            Password::new(std::env::var("ARBRITO_EXEC_PASSWORD").unwrap()),
            Some(TransactionCondition::Block(attempt.block.number.as_u64())),
        ))
        .gas(attempt.config.max_gas_usage)
        .gas_price(GasPrice::Value(gas_price))
        .confirmations(1)
        .nonce(attempt.block.nonce)
        .send()
        .await;

    match tx {
        Err(_) => log::info!(
            "{} {}",
            format_block_number(attempt.block.number),
            "Arbitrage execution failed".red().dimmed(),
        ),
        Ok(tx) => log::info!(
            "{} {} Transaction hash {}",
            format_block_number(attempt.block.number),
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

    let config = Config {
        target_weth_profit: U256::from(TARGET_WETH_PROFIT),
        expected_gas_usage: U256::from(EXPECTED_GAS_USAGE),
        max_gas_usage: U256::from(MAX_GAS_USAGE),
        min_gas_scale: MIN_GAS_SCALE,
    };

    let (tx, mut rx) = unbounded_channel::<ArbritageAttempt>();

    tokio::spawn(async move {
        let lock = Arc::new(Mutex::new(()));
        let mut block_number = U64::from(0);

        while let Some(attempt) = rx.recv().await {
            if attempt.block.number <= block_number {
                log::warn!(
                    "{} Dropped profitable attempt",
                    format_block_number(block_number)
                );
                continue;
            }

            block_number = attempt.block.number;

            match lock.clone().try_lock_owned() {
                Err(_) => log::info!(
                    "{} Dropped profitable attempt waiting for previous one",
                    format_block_number(block_number)
                ),
                Ok(guard) => {
                    tokio::spawn(execute(guard, attempt, arbrito.clone(), executor_address));
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
            let number = match head.ok().and_then(|h| h.number) {
                Some(number) => number,
                None => return (),
            };

            log::info!("{} New block header", format_block_number(number));
            let t = std::time::Instant::now();

            let block = Block::fetch(&web3, number, executor_address).await;

            let min_required_profit = config.target_weth_profit
                + (block.gas_price * config.min_gas_scale) * config.expected_gas_usage;

            log::info!(
                "{} Min required profit {} @ {} gwei",
                format_block_number(number),
                format_amount(&weth, min_required_profit),
                (block.gas_price * config.min_gas_scale) / U256::exp10(9)
            );

            let attempt_futs = arbritage_pairs.iter().map(|pair| {
                pair.attempts(config, block).map(|attempts| {
                    for attempt in &attempts {
                        if let ArbritageResult::NetProfit { .. } = attempt.result {
                            tx.send(attempt.clone()).expect("where's the executor at?");
                        }
                    }
                    attempts
                })
            });

            let attempts: Vec<_> = join_all(attempt_futs).await.into_iter().flatten().collect();

            let max_attempt = attempts
                .iter()
                .max_by(|a1, a2| a1.result.cmp(&a2.result))
                .expect("empty arbritage results");

            match max_attempt.result {
                ArbritageResult::NotProfit => {
                    log::info!("{} No profit found", format_block_number(number))
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
                        format_block_number(number),
                        format_amount(&max_attempt.tokens.0, amount),
                        max_attempt.tokens.1.symbol,
                        format_amount_colored(&weth, weth_profit),
                    );
                }
            }

            let mut not_profits_count = 0;
            let mut gross_profits_count = 0;
            let mut net_profits_count = 0;

            for attempt in attempts {
                match attempt.result {
                    ArbritageResult::NotProfit => not_profits_count += 1,
                    ArbritageResult::GrossProfit { .. } => gross_profits_count += 1,
                    ArbritageResult::NetProfit { .. } => net_profits_count += 1,
                }
            }

            log::info!(
                "{} Processed in {:.2} seconds ({} pairs | {} net + {} gross + {} not)",
                format_block_number(number),
                t.elapsed().as_secs_f64(),
                arbritage_pairs.len(),
                net_profits_count,
                gross_profits_count,
                not_profits_count
            );

            ()
        })
        .await;
}
