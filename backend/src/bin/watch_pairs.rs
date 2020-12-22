use colored::Colorize;
use ethcontract::{Account, BlockId, BlockNumber, GasPrice, Password, TransactionCondition};
use futures::{future::ready, FutureExt};
use itertools::Itertools;
use pooller::{
    blocks::Blocks,
    gen::{Arbrito, BalancerPool, UniswapPair},
    max_profit,
    txs::{UniswapSwap, UniswapSwapMatch},
    uniswap_out_given_in, Pairs, Token,
};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
};
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};
use web3::{
    futures::{future::join_all, StreamExt},
    transports::WebSocket,
    types::U64,
    types::{TransactionId, H160, U256},
    Web3,
};

const WEB3_ENDPOINT: &str = "ws://127.0.0.1:8546";
const WETH_ADDRESS: &str = "C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const ARBRITO_ADDRESS: &str = "3FE133c5b1Aa156bF7D8Cf3699794d09Ef911ec1";
const EXECUTOR_ADDRESS: &str = "Af43007aD675D6C72E96905cf4d8acB58ba0E041";
const UNISWAP_ROUTER_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
const TARGET_WETH_PROFIT: u128 = 10_000_000_000_000_000; // 0.01 eth
const EXPECTED_GAS_USAGE: u128 = 350_000;
const MAX_GAS_USAGE: u128 = 400_000;
const MIN_GAS_SCALE: u8 = 2;
const MAX_GAS_SCALE: u8 = 5;

fn format_amount_colored(token: &Token, amount: U256) -> String {
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

fn format_amount(token: &Token, amount: U256) -> String {
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
    id: BlockId,
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
    max_gas_scale: u8,
}

struct Context {
    block: Block,
    config: Config,
    pairs: HashMap<H160, UniswapPairResolved>,
    pools: HashMap<H160, BalancerPoolResolved>,
}

struct UniswapPairBase {
    contract: UniswapPair,
    address: H160,
    token0: H160,
}

struct UniswapPairResolved {
    reserve0: U256,
    reserve1: U256,
    token0: H160,
}

struct BalancerPoolBase {
    contract: BalancerPool,
    tokens: HashSet<H160>,
    address: H160,
}

struct BalancerPoolResolved {
    balances: HashMap<H160, U256>,
    swap_fee: U256,
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
struct ArbritageAttempt {
    pair: ArbritagePair,
    tokens: (Token, Token),
    result: ArbritageResult,
    config: Config,
    block: Block,
}

#[derive(Debug, Clone)]
struct ArbritagePair {
    balancer_pool: H160,
    uniswap_pair: H160,
    token0: Token,
    token1: Token,
    weth: Token,
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
            id: BlockId::Number(BlockNumber::Number(number)),
            gas_price: gas_price.expect("failed fetching gas_price"),
            balance: balance.expect("failed fetching balance"),
            nonce: nonce.expect("failed fetching nonce"),
            number,
        }
    }
}

impl UniswapPairBase {
    async fn resolve(&self, block: Block) -> (H160, UniswapPairResolved) {
        let req = self.contract.get_reserves().block(block.id);
        let (reserve0, reserve1, _) = req.call().await.expect("unable to fetch reserves");

        (
            self.address,
            UniswapPairResolved {
                reserve0: U256::from(reserve0),
                reserve1: U256::from(reserve1),
                token0: self.token0,
            },
        )
    }
}

impl BalancerPoolBase {
    async fn resolve(&self, block: Block) -> (H160, BalancerPoolResolved) {
        let req = self.contract.get_swap_fee().block(block.id);
        let swap_fee = req.call().await.expect("unable to fetch swap fee");

        let futs = self.tokens.iter().copied().map(|t| {
            let req = self.contract.get_balance(t).block(block.id);
            req.call().map(move |r| (t, r.expect("unable to balancer")))
        });

        let balances = join_all(futs).await.into_iter().collect();
        (self.address, BalancerPoolResolved { balances, swap_fee })
    }
}

impl ArbritagePair {
    fn run(&self, borrow_token: &Token, profit_token: &Token, ctx: &Context) -> ArbritageResult {
        let pair = ctx
            .pairs
            .get(&self.uniswap_pair)
            .expect("missing uniswap resolve");

        let pool = ctx
            .pools
            .get(&self.balancer_pool)
            .expect("missing balancer resolve");

        let (ro, ri) = if pair.token0 == borrow_token.address {
            (pair.reserve0, pair.reserve1)
        } else {
            (pair.reserve1, pair.reserve0)
        };

        let bi = pool
            .balances
            .get(&borrow_token.address)
            .expect("missing borrow token balance");

        let bo = pool
            .balances
            .get(&profit_token.address)
            .expect("missing profit token balance");

        let (borrow_amount, payback_amount, profit) =
            match max_profit(U256::from(ri), U256::from(ro), *bi, *bo, pool.swap_fee) {
                None => return ArbritageResult::NotProfit,
                Some(a) => a,
            };

        let weth_profit = if profit_token.address == self.weth.address {
            profit
        } else {
            let profit_pair_address = profit_token
                .weth_uniswap_pair
                .expect("required uniswap pair missing");

            let profit_pair = ctx.pairs.get(&profit_pair_address).unwrap();

            let (mut ro, mut ri) = if profit_pair.token0 == profit_token.address {
                (
                    U256::from(profit_pair.reserve1),
                    U256::from(profit_pair.reserve0),
                )
            } else {
                (
                    U256::from(profit_pair.reserve0),
                    U256::from(profit_pair.reserve1),
                )
            };

            if profit_pair_address == self.uniswap_pair {
                ri += payback_amount;
                ro -= borrow_amount;
            }

            uniswap_out_given_in(ri, ro, profit)
        };

        if weth_profit <= ctx.config.target_weth_profit {
            return ArbritageResult::GrossProfit {
                amount: borrow_amount,
                weth_profit,
            };
        }

        let max_gas_price = (ctx.block.balance / ctx.config.max_gas_usage)
            .min(ctx.block.gas_price * ctx.config.max_gas_scale);

        let min_gas_price = ctx.block.gas_price * ctx.config.min_gas_scale;

        let target_gas_price =
            (weth_profit - ctx.config.target_weth_profit) / ctx.config.expected_gas_usage;

        if max_gas_price < min_gas_price {
            log::warn!("max_gas_price < min_gas_price. attempt won't be correctly calculated");
            ArbritageResult::GrossProfit {
                amount: borrow_amount,
                weth_profit,
            }
        } else if target_gas_price < min_gas_price {
            ArbritageResult::GrossProfit {
                amount: borrow_amount,
                weth_profit,
            }
        } else {
            ArbritageResult::NetProfit {
                gas_price: target_gas_price.min(max_gas_price),
                amount: borrow_amount,
                weth_profit,
            }
        }
    }

    fn attempts(&self, ctx: &Context) -> Vec<ArbritageAttempt> {
        vec![
            ArbritageAttempt {
                pair: self.clone(),
                result: self.run(&self.token0, &self.token1, ctx),
                tokens: (self.token0.clone(), self.token1.clone()),
                config: ctx.config,
                block: ctx.block,
            },
            ArbritageAttempt {
                pair: self.clone(),
                result: self.run(&self.token1, &self.token0, ctx),
                tokens: (self.token1.clone(), self.token0.clone()),
                config: ctx.config,
                block: ctx.block,
            },
        ]
    }
}

async fn executor(
    arbrito: Arbrito,
    from_address: H160,
    execution_lock: Arc<Mutex<()>>,
    mut pending_txs_rx: mpsc::UnboundedReceiver<UniswapSwap>,
    mut execution_rx: mpsc::UnboundedReceiver<(ArbritageAttempt, Context)>,
) {
    let mut executing_attempt: Option<ArbritageAttempt> = None;
    loop {
        tokio::select! {
            pending_tx = pending_txs_rx.recv() => if let Some(swap) = pending_tx {
                if let Ok(_) = execution_lock.try_lock() {
                    continue;
                }

                if let Some(attempt) = &executing_attempt {
                    match swap.tokens_match(attempt.tokens.1.address, attempt.tokens.0.address) {
                        Some(UniswapSwapMatch::OppositeDirection) => log::warn!("Found :) concurrent Uniswap Swap: {:?}", swap.address),
                        Some(UniswapSwapMatch::SameDirection) => log::warn!("Found :( concurrent Uniswap Swap: {:?}", swap.address),
                        None => ()
                    };
                }
            },
            execution = execution_rx.recv() => if let Some((attempt, ctx)) = execution {
                if let Ok(guard) = execution_lock.clone().try_lock_owned() {
                    executing_attempt = Some(attempt.clone());
                    tokio::spawn(execute(guard, attempt, arbrito.clone(), from_address, ctx));
                }
            }
        }
    }
}

async fn execute(
    _: OwnedMutexGuard<()>,
    attempt: ArbritageAttempt,
    arbrito: Arbrito,
    from_address: H160,
    ctx: Context,
) {
    let (amount, gas_price) = match attempt.result {
        ArbritageResult::NetProfit {
            gas_price, amount, ..
        } => {
            log::debug!(
                "Token addresses = {} {}",
                attempt.tokens.0.address,
                attempt.tokens.1.address
            );
            log::debug!("UniswapPool = {}", attempt.pair.uniswap_pair);
            log::debug!("BalancerPool = {}", attempt.pair.balancer_pool);

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

    let pair = ctx
        .pairs
        .get(&attempt.pair.uniswap_pair)
        .expect("missing context uniswap pair");

    let pool = ctx
        .pools
        .get(&attempt.pair.balancer_pool)
        .expect("missing context balancer pool");

    let balance0 = *pool.balances.get(&attempt.pair.token0.address).unwrap();
    let balance1 = *pool.balances.get(&attempt.pair.token1.address).unwrap();

    let tx = arbrito
        .perform(
            borrow,
            amount,
            attempt.pair.uniswap_pair,
            attempt.pair.balancer_pool,
            attempt.pair.token0.address,
            attempt.pair.token1.address,
            pair.reserve0,
            pair.reserve1,
            balance0,
            balance1,
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

    let Pairs { tokens, pairs } = Pairs::read().expect("pairs reading failed");
    let tokens: Arc<HashMap<_, _>> = Arc::new(tokens.into_iter().map(|t| (t.address, t)).collect());

    let mut uniswap_pair_bases_addrs = HashSet::new();
    let mut uniswap_pair_bases: Vec<_> = pairs
        .iter()
        .unique_by(|pair| pair.uniswap_pair)
        .map(|pair| {
            uniswap_pair_bases_addrs.insert(pair.uniswap_pair);
            UniswapPairBase {
                contract: UniswapPair::at(&web3, pair.uniswap_pair),
                address: pair.uniswap_pair,
                token0: pair.token0,
            }
        })
        .collect();

    for (_, token) in tokens.iter() {
        if let Some(weth_uniswap_pair) = token.weth_uniswap_pair {
            if !uniswap_pair_bases_addrs.contains(&weth_uniswap_pair) {
                let contract = UniswapPair::at(&web3, weth_uniswap_pair);
                uniswap_pair_bases.push(UniswapPairBase {
                    token0: contract.token_0().call().await.unwrap(),
                    address: weth_uniswap_pair,
                    contract,
                });
            }
        }
    }

    let balancer_pair_bases: Vec<_> = pairs
        .iter()
        .group_by(|pair| pair.balancer_pool)
        .into_iter()
        .map(|(address, pairs)| {
            let tokens = pairs
                .into_iter()
                .flat_map(|pair| vec![pair.token0, pair.token1])
                .collect();

            BalancerPoolBase {
                contract: BalancerPool::at(&web3, address),
                address,
                tokens,
            }
        })
        .collect();

    let weth_address = H160::from_str(WETH_ADDRESS).expect("failed parsing weth address");
    let weth = tokens.get(&weth_address).expect("where's my weth, boy?");

    let arbrito_address = H160::from_str(ARBRITO_ADDRESS).expect("failed parsing arbrito address");
    let arbrito = Arbrito::at(&web3, arbrito_address);

    let executor_address =
        H160::from_str(EXECUTOR_ADDRESS).expect("failed parsing executor address");

    let uniswap_router_address =
        H160::from_str(UNISWAP_ROUTER_ADDRESS).expect("failed parsing uniswap router address");

    let config = Config {
        target_weth_profit: U256::from(TARGET_WETH_PROFIT),
        expected_gas_usage: U256::from(EXPECTED_GAS_USAGE),
        max_gas_usage: U256::from(MAX_GAS_USAGE),
        min_gas_scale: MIN_GAS_SCALE,
        max_gas_scale: MAX_GAS_SCALE,
    };

    let execution_lock = Arc::new(Mutex::new(()));
    let (execution_tx, execution_rx) = mpsc::unbounded_channel();
    let (pending_txs_tx, pending_txs_rx) = mpsc::unbounded_channel();

    tokio::spawn(executor(
        arbrito.clone(),
        executor_address,
        execution_lock.clone(),
        pending_txs_rx,
        execution_rx,
    ));

    let arbritage_pairs: Vec<_> = pairs
        .into_iter()
        .map(|pair| ArbritagePair {
            token0: tokens.get(&pair.token0).expect("unknown token").clone(),
            token1: tokens.get(&pair.token1).expect("unknown token").clone(),
            balancer_pool: pair.balancer_pool,
            uniswap_pair: pair.uniswap_pair,
            weth: weth.clone(),
        })
        .collect();

    let web32 = web3.clone();
    let tokens2 = tokens.clone();
    tokio::spawn(
        web3.eth_subscribe()
            .subscribe_new_pending_transactions()
            .await
            .expect("failed subscribing to new pending transactions")
            .filter_map(|res| async move { Result::ok(res) })
            .filter_map(move |tx_hash| {
                web32
                    .eth()
                    .transaction(TransactionId::Hash(tx_hash))
                    .map(Result::ok)
                    .map(Option::flatten)
            })
            .for_each(move |tx: web3::types::Transaction| {
                let to = match tx.to {
                    None => return ready(()),
                    Some(to) => to,
                };

                if to != uniswap_router_address {
                    return ready(());
                }

                if let Some(swap) = UniswapSwap::from_transaction(&tx, &tokens2) {
                    log::debug!("Uniswap {:?} {:?}", swap, tx.hash);
                    pending_txs_tx.send(swap).expect("Pending txs rx died");
                }

                ready(())
            }),
    );

    let blocks = Blocks::new(&web3, executor_address).await;

    loop {
        let block = blocks.next().await;

        log::info!("{} New block header", format_block_number(block.number));
        if let Err(_) = execution_lock.try_lock() {
            log::info!(
                "{} Waiting on previous execution",
                format_block_number(block.number)
            );
            continue;
        };

        let t = std::time::Instant::now();
        let block = Block::fetch(&web3, block.number, executor_address).await;

        let futs = uniswap_pair_bases.iter().map(|pair| pair.resolve(block));
        let uniswap_pair_resolves: HashMap<_, _> = join_all(futs).await.into_iter().collect();

        let futs = balancer_pair_bases.iter().map(|pair| pair.resolve(block));
        let balancer_pool_resolves: HashMap<_, _> = join_all(futs).await.into_iter().collect();

        let context = Context {
            pools: balancer_pool_resolves,
            pairs: uniswap_pair_resolves,
            config,
            block,
        };

        let min_required_profit = config.target_weth_profit
            + (block.gas_price * config.min_gas_scale) * config.expected_gas_usage;

        log::info!(
            "{} Min required profit {} @ {} gwei",
            format_block_number(block.number),
            format_amount(&weth, min_required_profit),
            (block.gas_price * config.min_gas_scale) / U256::exp10(9)
        );

        let attempts: Vec<_> = arbritage_pairs
            .iter()
            .map(|pair| pair.attempts(&context))
            .flatten()
            .collect();

        let mut not_profits_count = 0;
        let mut gross_profits_count = 0;
        let mut net_profits_count = 0;

        for attempt in &attempts {
            match attempt.result {
                ArbritageResult::NotProfit => not_profits_count += 1,
                ArbritageResult::GrossProfit { .. } => gross_profits_count += 1,
                ArbritageResult::NetProfit { .. } => net_profits_count += 1,
            }
        }

        let max_attempt = attempts
            .into_iter()
            .max_by(|a1, a2| a1.result.cmp(&a2.result))
            .expect("empty arbritage results");

        match max_attempt.result {
            ArbritageResult::NotProfit => {
                log::info!("{} All attempts suck", format_block_number(block.number))
            }
            ArbritageResult::GrossProfit {
                weth_profit,
                amount,
            } => {
                log::info!(
                    "{} Best attempt found: borrow {} for {} profit ({})",
                    format_block_number(block.number),
                    format_amount(&max_attempt.tokens.0, amount),
                    max_attempt.tokens.1.symbol,
                    format_amount_colored(&weth, weth_profit),
                );
            }
            ArbritageResult::NetProfit {
                weth_profit,
                gas_price,
                amount,
                ..
            } => {
                log::info!(
                    "{} {}: borrow {} for {} profit ({} @ {} gwei)",
                    format_block_number(block.number),
                    "Executing best attempt".bold().underline(),
                    format_amount(&max_attempt.tokens.0, amount),
                    max_attempt.tokens.1.symbol,
                    format_amount_colored(&weth, weth_profit),
                    gas_price / U256::exp10(9)
                );

                if let Err(_) = execution_tx.send((max_attempt, context)) {
                    panic!("where's my executor at?");
                }
            }
        }

        log::info!(
            "{} Processed in {:.2} seconds ({} pairs | {} net + {} gross + {} not)",
            format_block_number(block.number),
            t.elapsed().as_secs_f64(),
            arbritage_pairs.len(),
            net_profits_count,
            gross_profits_count,
            not_profits_count
        );
    }
}
