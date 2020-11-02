use pooller::{
    gen::{Balancer, Uniswap},
    Pairs, Token,
};
use std::{collections::HashMap, str::FromStr};
use web3::{
    futures::{future::join_all, StreamExt},
    transports::WebSocket,
    types::{H160, U256},
    Web3,
};

const UNISWAP_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
const WEB3_ENDPOINT: &str = "ws://127.0.0.1:8546";

fn format_amount(token: &Token, amount: U256) -> String {
    let d = U256::exp10(token.decimals as usize);
    format!(
        "{}.{:03$} {}",
        (amount / d).as_u128(),
        (amount % d).as_u128(),
        token.symbol,
        token.decimals as usize,
    )
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum ArbritageResult {
    Deficit(U256),
    Profit(U256),
}

struct ArbritageAttempt<'a> {
    tokens: (&'a Token, &'a Token),
    result: ArbritageResult,
    amount: U256,
}

struct ArbritagePair<'a> {
    balancer: Balancer,
    uniswap: Uniswap,
    token0: &'a Token,
    token1: &'a Token,
}

impl<'a> ArbritagePair<'a> {
    async fn attempt(&'a self, source: &'a Token, target: &'a Token) -> ArbritageAttempt<'a> {
        let amount = U256::exp10(source.decimals as usize);

        let uniswap_amount = self
            .uniswap
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

        let result = if balancer_amount > uniswap_amount {
            ArbritageResult::Profit(balancer_amount - uniswap_amount)
        } else {
            ArbritageResult::Deficit(uniswap_amount - balancer_amount)
        };

        ArbritageAttempt {
            tokens: (source, target),
            result,
            amount,
        }
    }

    async fn attempts(&'a self) -> Vec<ArbritageAttempt<'a>> {
        vec![
            self.attempt(&self.token0, &self.token1).await,
            self.attempt(&self.token1, &self.token0).await,
        ]
    }
}

#[tokio::main]
async fn main() {
    let web3 = Web3::new(WebSocket::new(WEB3_ENDPOINT).await.expect("ws failed"));
    let uniswap_router = H160::from_str(UNISWAP_ADDRESS).expect("failed parsing uniswap address");

    let Pairs { tokens, pairs } = Pairs::read().expect("pairs reading failed");
    let tokens: HashMap<_, _> = tokens.into_iter().map(|t| (t.address, t)).collect();

    let arbritage_pairs: Vec<_> = pairs
        .into_iter()
        .map(|pair| ArbritagePair {
            balancer: Balancer::at(&web3, pair.balancer),
            uniswap: Uniswap::at(&web3, uniswap_router),
            token0: tokens.get(&pair.token0).expect("unknown token"),
            token1: tokens.get(&pair.token1).expect("unknown token"),
        })
        .collect();

    web3.eth_subscribe()
        .subscribe_new_heads()
        .await
        .expect("failed subscribing to new heads")
        .for_each(|head| async {
            match head.ok().and_then(|h| h.number) {
                Some(number) => println!("\n#{}", number),
                None => return (),
            };

            let t = std::time::Instant::now();

            let attempt_futs = arbritage_pairs.iter().map(ArbritagePair::attempts);
            let mut attempts: Vec<_> = join_all(attempt_futs).await.into_iter().flatten().collect();
            attempts.sort_unstable_by(|a1, a2| a1.result.cmp(&a2.result));

            let mut deficit_count = 0;
            for attempt in attempts {
                match attempt.result {
                    ArbritageResult::Deficit(_) => deficit_count += 1,
                    ArbritageResult::Profit(amount) => {
                        let (token0, token1) = attempt.tokens;
                        println!(
                            "🟢 {} -> {}",
                            format_amount(token0, attempt.amount),
                            format_amount(token1, amount)
                        );
                    }
                }
            }

            println!("omitting {} deficits", deficit_count);
            println!("took {:.2} seconds", t.elapsed().as_secs_f64());

            ()
        })
        .await;
}
