use crate::{
    gen::{Balancer, Uniswap},
    resolve_pools::BalancerPool,
};
use ethcontract::{errors::MethodError, futures::StreamExt};
use std::{error::Error, fmt::Debug, str::FromStr};
use web3::types::{H160, U256};

const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const UNISWAP_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

const ETH_INPUT: &str = "8AC7230489E80000"; // 10 eth
const ETH_MIN_PROFIT: &str = "16345785D8A0000"; // 0.1 eth

enum ArbritageResult {
    Deficit(U256),
    Neutral(U256),
    Profit(U256),
}

fn u256_to_pretty_eth_string(n: U256) -> String {
    let integer = n / 10u64.pow(18);
    let decimals = n / 10u64.pow(14) % 10u64.pow(4);

    format!("Ξ {}.{}", integer, decimals)
}

impl Debug for ArbritageResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ArbritageResult::Deficit(p) => format!("🔴 {}", u256_to_pretty_eth_string(*p)),
            ArbritageResult::Neutral(p) => format!("🟡 {}", u256_to_pretty_eth_string(*p)),
            ArbritageResult::Profit(p) => format!("🟢 {}", u256_to_pretty_eth_string(*p)),
        };

        f.write_str(&message)
    }
}

async fn uniswap_swap(
    uniswap: &Uniswap,
    source: H160,
    target: H160,
    amount: U256,
) -> Result<U256, MethodError> {
    uniswap
        .get_amounts_out(amount, vec![source, target])
        .call()
        .await
        .map(|a| a[1])
}

async fn balancer_swap(
    balancer: &Balancer,
    source: H160,
    target: H160,
    amount: U256,
) -> Result<U256, MethodError> {
    balancer
        .calc_out_given_in(
            balancer.get_balance(source).call().await?,
            balancer.get_denormalized_weight(source).call().await?,
            balancer.get_balance(target).call().await?,
            balancer.get_denormalized_weight(target).call().await?,
            amount,
            balancer.get_swap_fee().call().await?,
        )
        .call()
        .await
}

async fn arbritage(
    uniswap: &Uniswap,
    balancer: &Balancer,
    weth_address: H160,
    token_address: H160,
    eth_input: U256,
    eth_min_profit: U256,
) -> Result<ArbritageResult, MethodError> {
    let token_uniswap_output =
        uniswap_swap(&uniswap, weth_address, token_address, eth_input).await?;
    let token_balancer_output =
        balancer_swap(&balancer, weth_address, token_address, eth_input).await?;

    let weth_output = if token_uniswap_output > token_balancer_output {
        balancer_swap(&balancer, token_address, weth_address, token_uniswap_output).await
    } else {
        uniswap_swap(&uniswap, token_address, weth_address, token_balancer_output).await
    }?;

    Ok(if weth_output < eth_input {
        ArbritageResult::Deficit(eth_input - weth_output)
    } else if weth_output - eth_input < eth_min_profit {
        ArbritageResult::Neutral(weth_output - eth_input)
    } else {
        ArbritageResult::Profit(weth_output - eth_input)
    })
}

pub async fn watch_pools(
    web_endpoint: String,
    balancer_pools: Vec<BalancerPool>,
) -> Result<(), Box<dyn Error>> {
    let web3 = web3::Web3::new(web3::transports::WebSocket::new(&web_endpoint).await?);

    let uniswap = Uniswap::at(&web3, H160::from_str(UNISWAP_ADDRESS)?);
    let weth_address = H160::from_str(WETH_ADDRESS)?;

    let eth_input = U256::from_str(ETH_INPUT)?;
    let eth_min_profit = U256::from_str(ETH_MIN_PROFIT)?;

    let mut balancers: Vec<(Balancer, H160, &str)> = vec![];
    for pool in &balancer_pools {
        let pool_address = H160::from_str(&pool.address.strip_prefix("0x").unwrap())?;
        balancers.push((
            Balancer::at(&web3, pool_address),
            H160::from_str(&pool.token.address.strip_prefix("0x").unwrap())?,
            &pool.token.name,
        ));
    }

    web3.eth_subscribe()
        .subscribe_new_heads()
        .await?
        .for_each(|head| async {
            let number = match head.ok().and_then(|h| h.number) {
                None => return (),
                Some(a) => a,
            };

            println!("\n#{}", number);

            let t = std::time::Instant::now();

            let mut futs = vec![];
            for (balancer, token_address, name) in &balancers {
                let u = &uniswap;

                futs.push(async move {
                    let result = arbritage(
                        u,
                        balancer,
                        weth_address,
                        *token_address,
                        eth_input,
                        eth_min_profit,
                    )
                    .await;

                    println!("#{}\t{:?}\t{}", balancer.address(), result, name);
                });
            }

            futures::future::join_all(futs).await;
            println!("took {:.2} seconds", t.elapsed().as_secs_f64());

            ()
        })
        .await;

    Ok(())
}
