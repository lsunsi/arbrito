use colored::{ColoredString, Colorize};
use ethcontract::{Account, BlockNumber, GasPrice, Password, TransactionCondition};
use pooller::{
    gen::{Arbrito, Balancer, Uniswap, UniswapPair},
    Pairs, Token,
};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};
use web3::{
    futures::{future::join_all, StreamExt},
    transports::WebSocket,
    types::{H160, U256},
    Web3,
};

const UNISWAP_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const ARBRITO_ADDRESS: &str = "e96B6680a8ef1D8d561171948226bc3A133fA56D";
const EXECUTOR_ADDRESS: &str = "Af43007aD675D6C72E96905cf4d8acB58ba0E041";
const WEB3_ENDPOINT: &str = "ws://127.0.0.1:8546";
const GAS_USAGE: u128 = 400_000;
const GAS_SCALE: u8 = 2;

fn format_amount(token: &Token, amount: U256) -> String {
    let d = U256::exp10(token.decimals as usize);
    format!(
        "{: <4} {}.{:03$}",
        token.symbol,
        (amount / d).as_u128(),
        (amount % d).as_u128(),
        token.decimals as usize,
    )
}

fn color_eth_output(amount: U256, str: String) -> ColoredString {
    if amount >= U256::exp10(18) {
        str.bright_green().bold().italic().underline()
    } else if amount >= U256::exp10(17) {
        str.bright_green().bold()
    } else if amount >= U256::exp10(16) {
        str.green()
    } else if amount >= U256::exp10(15) {
        str.yellow().dimmed()
    } else {
        str.dimmed()
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum ArbritageResult {
    Deficit,
    Profit(U256, U256),
}

struct ArbritageAttempt {
    pair: ArbritagePair,
    tokens: (Token, Token),
    result: ArbritageResult,
    amount: U256,
}

#[derive(Clone)]
struct ArbritagePair {
    uniswap_router: Uniswap,
    uniswap_pair: UniswapPair,
    balancer: Balancer,
    token0: Token,
    token1: Token,
    weth: Token,
}

impl ArbritagePair {
    async fn run(&self, source: &Token, target: &Token, amount: U256) -> ArbritageResult {
        let uniswap_amount = self
            .uniswap_router
            .get_amounts_in(amount, vec![target.address, source.address])
            .call()
            .await
            .expect("uniswap get_amounts_in failed")[0];

        let balancer_amount = self
            .balancer
            .calc_out_given_in(
                self.balancer
                    .get_balance(source.address)
                    .call()
                    .await
                    .expect("balancer get_balance(source) failed"),
                self.balancer
                    .get_denormalized_weight(source.address)
                    .call()
                    .await
                    .expect("balancer get_denormalized_weight(source) failed"),
                self.balancer
                    .get_balance(target.address)
                    .call()
                    .await
                    .expect("balancer get_balance(target) failed"),
                self.balancer
                    .get_denormalized_weight(target.address)
                    .call()
                    .await
                    .expect("balancer get_denormalized_weight(target) failed"),
                amount,
                self.balancer
                    .get_swap_fee()
                    .call()
                    .await
                    .expect("balancer get_swap_fee failed"),
            )
            .call()
            .await
            .expect("balancer calc_out_given_in failed");

        if balancer_amount < uniswap_amount {
            ArbritageResult::Deficit
        } else if target.address == self.weth.address {
            let profit = balancer_amount - uniswap_amount;
            ArbritageResult::Profit(profit, profit)
        } else {
            let target_profit = balancer_amount - uniswap_amount;
            let weth_profit = self
                .uniswap_router
                .get_amounts_out(
                    balancer_amount - uniswap_amount,
                    vec![target.address, self.weth.address],
                )
                .call()
                .await
                .expect("uniswap get_amounts_out failed")[1];

            ArbritageResult::Profit(weth_profit, target_profit)
        }
    }

    async fn attempt(&self, source: Token, target: Token) -> ArbritageAttempt {
        let one = if source.address == self.weth.address {
            U256::exp10(18)
        } else {
            self.uniswap_router
                .get_amounts_out(U256::exp10(18), vec![self.weth.address, source.address])
                .call()
                .await
                .expect("uniswap get_amounts_out failed")[1]
        };

        let mut amount = one;
        let mut result = self.run(&source, &target, amount).await;

        for i in 2..=10 {
            let amount2 = one * i;
            let result2 = self.run(&source, &target, amount2).await;

            if result < result2 {
                amount = amount2;
                result = result2;
            }
        }

        ArbritageAttempt {
            pair: self.clone(),
            tokens: (source, target),
            amount,
            result,
        }
    }

    async fn attempts(&self) -> Vec<ArbritageAttempt> {
        vec![
            self.attempt(self.token0.clone(), self.token1.clone()).await,
            self.attempt(self.token1.clone(), self.token0.clone()).await,
        ]
    }
}

async fn execute(
    _: OwnedMutexGuard<()>,
    attempt: ArbritageAttempt,
    arbrito: Arbrito,
    block_number: u64,
    from_address: H160,
    gas_usage: U256,
    gas_price: U256,
    nonce: U256,
) {
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
            attempt.amount,
            attempt.pair.uniswap_pair.address(),
            attempt.pair.balancer.address(),
            attempt.pair.token0.address,
            attempt.pair.token1.address,
            U256::from(reserve0),
            U256::from(reserve1),
            U256::from(block_number + 1),
        )
        .from(Account::Locked(
            from_address,
            Password::new(std::env::var("ARBRITO_EXEC_PASSWORD").unwrap()),
            Some(TransactionCondition::Block(block_number)),
        ))
        .gas(gas_usage)
        .gas_price(GasPrice::Value(gas_price))
        .confirmations(1)
        .nonce(nonce)
        .send()
        .await;

    match tx {
        Ok(tx) => println!(
            "{} {:?}",
            "HUGE SUCCESS!".bright_green().bold().italic().underline(),
            tx.hash()
        ),
        Err(_) => println!("minor failure"),
    }
}

#[tokio::main]
async fn main() {
    let web3 = Web3::new(WebSocket::new(WEB3_ENDPOINT).await.expect("ws failed"));

    let weth_address = H160::from_str(WETH_ADDRESS).expect("failed parsing weth address");
    let uniswap_router = H160::from_str(UNISWAP_ADDRESS).expect("failed parsing uniswap address");
    let uniswap = Uniswap::at(&web3, uniswap_router);

    let Pairs { tokens, pairs } = Pairs::read().expect("pairs reading failed");
    let tokens: HashMap<_, _> = tokens.into_iter().map(|t| (t.address, t)).collect();
    let weth = tokens.get(&weth_address).expect("where's my weth, boy?");

    let arbrito_address = H160::from_str(ARBRITO_ADDRESS).expect("failed parsing arbrito address");
    let arbrito = Arbrito::at(&web3, arbrito_address);

    let executor_address =
        H160::from_str(EXECUTOR_ADDRESS).expect("failed parsing executor address");

    let gas_usage = U256::from(GAS_USAGE);

    let execution_lock = Arc::new(Mutex::new(()));

    let arbritage_pairs: Vec<_> = pairs
        .into_iter()
        .map(|pair| ArbritagePair {
            token0: tokens.get(&pair.token0).expect("unknown token").clone(),
            token1: tokens.get(&pair.token1).expect("unknown token").clone(),
            balancer: Balancer::at(&web3, pair.balancer),
            uniswap_pair: UniswapPair::at(&web3, pair.uniswap),
            uniswap_router: uniswap.clone(),
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

            println!("\n#{}", block_number);

            let guard = match execution_lock.clone().try_lock_owned() {
                Ok(guard) => guard,
                Err(_) => {
                    println!("waiting");
                    return ();
                }
            };

            let gas_price = web3
                .eth()
                .gas_price()
                .await
                .expect("failed getting gas price")
                * GAS_SCALE;

            let cost = gas_price * gas_usage;

            let t = std::time::Instant::now();

            let attempt_futs = arbritage_pairs.iter().map(ArbritagePair::attempts);
            let mut attempts: Vec<_> = join_all(attempt_futs).await.into_iter().flatten().collect();
            attempts.sort_unstable_by(|a1, a2| a2.result.cmp(&a1.result));

            let mut deficit_count = 0;
            for attempt in &attempts {
                match attempt.result {
                    ArbritageResult::Deficit => deficit_count += 1,
                    ArbritageResult::Profit(weth_amount, target_amount) => {
                        let (token_from, token_to) = &attempt.tokens;
                        println!(
                            " {0: <30} <-> {1: <30} ~> {2: <30}",
                            format_amount(token_from, attempt.amount),
                            format_amount(token_to, target_amount),
                            color_eth_output(weth_amount, format_amount(weth, weth_amount))
                        );
                    }
                }
            }

            println!(
                "cost {} @ {} gwei",
                format_amount(&weth, cost),
                gas_price / U256::exp10(9)
            );
            println!("omitting {} deficits", deficit_count);
            println!("took {:.2} seconds", t.elapsed().as_secs_f64());

            if attempts.len() > 0 {
                let attempt = attempts.remove(0);
                if let ArbritageResult::Profit(weth_amount, _) = attempt.result {
                    if weth_amount >= cost {
                        let nonce = web3
                            .eth()
                            .transaction_count(
                                executor_address,
                                Some(BlockNumber::Number(block_number)),
                            )
                            .await
                            .expect("failed fetching nonce");

                        tokio::spawn(execute(
                            guard,
                            attempt,
                            arbrito.clone(),
                            block_number.as_u64(),
                            executor_address,
                            gas_usage,
                            gas_price,
                            nonce,
                        ));
                    }
                }
            }

            ()
        })
        .await;
}
