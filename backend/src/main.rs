mod gen;

use ethcontract::{errors::MethodError, futures::StreamExt};
use gen::{Balancer, Uniswap};
use std::{error::Error, str::FromStr};
use web3::types::{H160, U256};

const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const LINK_ADDRESS: &str = "514910771af9ca656af840dff83e8264ecf986ca";

const UNISWAP_ADDRESS: &str = "7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
const BALANCER_ADDRESS: &str = "e010fcda8894c16a8acfef7b37741a760faeddc4";

const ETH_INPUT: &str = "8AC7230489E80000"; // 10 eth
const ETH_MIN_PROFIT: &str = "16345785D8A0000"; // 0.1 eth

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

#[derive(Debug)]
enum ArbritageResult {
    NoProfit(U256, U256),
    ProfitButNo(U256, U256),
    YayProfit(U256, U256),
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
        ArbritageResult::NoProfit(weth_output, eth_input - weth_output)
    } else {
        let profit = weth_output - eth_input;
        if profit < eth_min_profit {
            ArbritageResult::ProfitButNo(weth_output, profit)
        } else {
            ArbritageResult::YayProfit(weth_output, profit)
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let web_endpoint = std::env::var("WEB3_ENDPOINT").map_err(|_| "Missing WEB3_ENDPOINT")?;
    let web3 = web3::Web3::new(web3::transports::WebSocket::new(&web_endpoint).await?);

    let uniswap = Uniswap::at(&web3, H160::from_str(UNISWAP_ADDRESS)?);
    let balancer = Balancer::at(&web3, H160::from_str(BALANCER_ADDRESS)?);

    let weth_address = H160::from_str(WETH_ADDRESS)?;
    let token_address = H160::from_str(LINK_ADDRESS)?;

    let eth_input = U256::from_str(ETH_INPUT)?;
    let eth_min_profit = U256::from_str(ETH_MIN_PROFIT)?;

    web3.eth_subscribe()
        .subscribe_new_heads()
        .await?
        .for_each(|head| async {
            let number = match head.ok().and_then(|h| h.number) {
                None => return (),
                Some(a) => a,
            };

            let result = arbritage(
                &uniswap,
                &balancer,
                weth_address,
                token_address,
                eth_input,
                eth_min_profit,
            )
            .await;

            let result = match result {
                Err(_) => return (),
                Ok(a) => a,
            };

            println!("#{:?} : {:?}", number, result);

            ()
        })
        .await;

    Ok(())
}
