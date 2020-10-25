use crate::{
    gen::{Balancer, Uniswap},
    resolve_pools::BalancerPool,
};
use ethcontract::{errors::MethodError, futures::StreamExt};
use std::{error::Error, fmt::Debug, str::FromStr};
use web3::types::{H160, U256};

const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const UNISWAP_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

const ETH_INPUT_TOLERANCE: &str = "2386F26FC10000"; // 0.01 eth
const ETH_INPUT_HI: &str = "8AC7230489E80000"; // 10 eth
const ETH_INPUT_LO: &str = "16345785D8A0000"; // 0.1 eth

struct ArbritageResult {
    input: U256,
    delta: U256,
    profit: bool,
}

fn u256_to_pretty_eth_string(n: U256) -> String {
    let int = (n / 10u64.pow(14)).as_u64();
    format!(
        "Ξ  {}.{:04} ({})",
        int / 10u64.pow(4),
        int % 10u64.pow(4),
        n
    )
}

impl Debug for ArbritageResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "{} : {} Δ {} % {}",
            if self.profit { "🟢" } else { "🔴" },
            u256_to_pretty_eth_string(self.input),
            u256_to_pretty_eth_string(self.delta),
            (self.input + self.delta) / self.input
        ))
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

    Ok(if weth_output <= eth_input {
        ArbritageResult {
            input: eth_input,
            delta: eth_input - weth_output,
            profit: false,
        }
    } else {
        ArbritageResult {
            input: eth_input,
            delta: weth_output - eth_input,
            profit: true,
        }
    })
}

async fn arbritage_search(
    uniswap: &Uniswap,
    balancer: &Balancer,
    weth_address: H160,
    token_address: H160,
    eth_input_tolerance: U256,
    mut eth_input_lo: U256,
    mut eth_input_hi: U256,
) -> Result<ArbritageResult, MethodError> {
    let mut acc = arbritage(uniswap, balancer, weth_address, token_address, eth_input_lo).await?;

    if !acc.profit {
        return Ok(acc);
    }

    while eth_input_hi - eth_input_lo > eth_input_tolerance {
        let eth_input = (eth_input_hi + eth_input_lo) / 2;
        let acc2 = arbritage(uniswap, balancer, weth_address, token_address, eth_input).await?;

        if acc2.profit && acc2.delta > acc.delta {
            eth_input_lo = eth_input;
            acc = acc2;
        } else {
            eth_input_hi = eth_input;
        }
    }

    Ok(acc)
}

pub async fn watch_pools(
    web_endpoint: String,
    balancer_pools: Vec<BalancerPool>,
) -> Result<(), Box<dyn Error>> {
    let web3 = web3::Web3::new(web3::transports::WebSocket::new(&web_endpoint).await?);

    let uniswap = Uniswap::at(&web3, H160::from_str(UNISWAP_ADDRESS)?);
    let weth_address = H160::from_str(WETH_ADDRESS)?;

    let eth_input_lo = U256::from_str(ETH_INPUT_LO)?;
    let eth_input_hi = U256::from_str(ETH_INPUT_HI)?;
    let eth_input_tolerance = U256::from_str(ETH_INPUT_TOLERANCE)?;

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
                    let result = arbritage_search(
                        u,
                        balancer,
                        weth_address,
                        *token_address,
                        eth_input_tolerance,
                        eth_input_lo,
                        eth_input_hi,
                    )
                    .await
                    .unwrap();

                    if result.profit {
                        println!("#{}\t{:?}\t{}", balancer.address(), result, name);
                    }
                });
            }

            futures::future::join_all(futs).await;
            println!("took {:.2} seconds", t.elapsed().as_secs_f64());

            ()
        })
        .await;

    Ok(())
}
