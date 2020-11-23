use bigdecimal::{BigDecimal, BigDecimal as BigInt, FromPrimitive, ToPrimitive};
use futures::{Future, TryFutureExt};
use graphql_client::{GraphQLQuery, Response};
use pooller::{Pair, Pairs, Token};
use reqwest::Client;
use std::{fmt::Debug, str::FromStr, time::Duration};
use tokio::time::delay_for;
use web3::types::H160;

const UNISWAP_URL: &str = "https://api.thegraph.com/subgraphs/name/ianlapham/uniswapv2";
const BALANCER_URL: &str = "https://api.thegraph.com/subgraphs/name/balancer-labs/balancer-beta";

const UNISWAP_MIN_ETH_RESERVE: u64 = 10_000;
const BALANCER_MIN_LIQUIDITY: u64 = 100_000;
const BALANCER_MAX_SWAP_FEE: f64 = 0.01;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/uniswap_schema.graphql",
    query_path = "graphql/uniswap_query.graphql"
)]
struct UniswapGetPairs;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/balancer_schema.graphql",
    query_path = "graphql/balancer_query.graphql"
)]
struct BalancerGetPools;

fn parse_address(addr: String) -> H160 {
    H160::from_str(addr.strip_prefix("0x").expect("missing prefix")).expect("h160 parsing failed")
}

async fn send<T, E: Debug, Fut: Future<Output = Result<Response<T>, E>>>(
    func: &dyn Fn() -> Fut,
) -> T {
    loop {
        match func().await {
            Err(e) => {
                delay_for(Duration::from_millis(1000)).await;
                log::warn!("retrying due to network {:?}", e);
            }
            Ok(Response { errors, data }) => {
                if let Some(errors) = errors {
                    delay_for(Duration::from_millis(1000)).await;
                    log::warn!("retrying due to application {:?}", errors);
                } else {
                    return data.expect("how can error and data both be none?");
                }
            }
        }
    }
}

async fn uniswap_pairs(client: &Client, min_reserve_eth: BigDecimal) -> Vec<(H160, Token, Token)> {
    log::info!("uniswap_pairs | started");
    let query = UniswapGetPairs::build_query(uniswap_get_pairs::Variables { min_reserve_eth });

    let data: uniswap_get_pairs::ResponseData = send(&|| {
        client
            .post(UNISWAP_URL)
            .json(&query)
            .send()
            .and_then(|a| a.json())
    })
    .await;

    let mut pairs = vec![];
    let parse_decimals = |bigdec: BigDecimal| bigdec.to_usize().expect("decimals parsing failed");

    if data.pairs.len() == 1000 {
        log::warn!("possible pagination limiting");
    }

    for pair in data.pairs {
        pairs.push((
            parse_address(pair.id),
            Token {
                symbol: pair.token0.symbol,
                address: parse_address(pair.token0.id),
                decimals: parse_decimals(pair.token0.decimals),
            },
            Token {
                symbol: pair.token1.symbol,
                address: parse_address(pair.token1.id),
                decimals: parse_decimals(pair.token1.decimals),
            },
        ));
    }

    log::info!("uniswap_pairs | {} pairs fetched", pairs.len());
    pairs
}

async fn balancer_pools(
    client: &Client,
    uniswap_pairs: &[(H160, Token, Token)],
    min_liquidity: BigDecimal,
    max_swap_fee: BigDecimal,
) -> Vec<Vec<H160>> {
    let mut pools = vec![];
    let mut count = 0;

    for (index, (_, token0, token1)) in uniswap_pairs.into_iter().enumerate() {
        log::info!(
            "balancer_pools | started {} / {}",
            index + 1,
            uniswap_pairs.len()
        );

        let query = BalancerGetPools::build_query(balancer_get_pools::Variables {
            min_liquidity: min_liquidity.clone(),
            max_swap_fee: max_swap_fee.clone(),
            tokens: vec![
                format!("{:?}", token0.address),
                format!("{:?}", token1.address),
            ],
        });

        let data: balancer_get_pools::ResponseData = send(&|| {
            client
                .post(BALANCER_URL)
                .json(&query)
                .send()
                .and_then(|a| a.json())
        })
        .await;

        pools.push(
            data.pools
                .into_iter()
                .map(|p| {
                    count += 1;
                    parse_address(p.id)
                })
                .collect(),
        );
    }

    log::info!("balancer_pools | {} pools fetched", count);
    pools
}

fn build_pairs(uniswap_pairs: Vec<(H160, Token, Token)>, balancer_pools: Vec<Vec<H160>>) -> Pairs {
    let mut tokens = vec![];
    let mut pairs = vec![];

    for ((uniswap, token0, token1), balancers) in uniswap_pairs.into_iter().zip(balancer_pools) {
        for balancer in balancers {
            pairs.push(Pair {
                token0: token0.address.clone(),
                token1: token1.address.clone(),
                balancer: balancer.clone(),
                uniswap: uniswap.clone(),
            });
        }

        tokens.push(token0);
        tokens.push(token1);
    }

    tokens.sort_unstable_by_key(|t| t.address);
    tokens.dedup_by_key(|t| t.address);

    pairs.sort_unstable_by(|p1, p2| p1.cmp(&p2));
    pairs.dedup_by(|p1, p2| p1 == p2);

    Pairs { tokens, pairs }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let uniswap_min_eth_reserve = BigDecimal::from_u64(UNISWAP_MIN_ETH_RESERVE)
        .expect("UNISWAP_MIN_ETH_RESERVE parsing failed");

    let balancer_min_liquidity = BigDecimal::from_u64(BALANCER_MIN_LIQUIDITY)
        .expect("BALANCER_MIN_LIQUIDITY parsing failed");

    let balancer_max_swap_fee =
        BigDecimal::from_f64(BALANCER_MAX_SWAP_FEE).expect("BALANCER_MAX_SWAP_FEE parsing failed");

    let client = reqwest::Client::new();

    let uniswap_pairs = uniswap_pairs(&client, uniswap_min_eth_reserve).await;
    let balancer_pools = balancer_pools(
        &client,
        &uniswap_pairs,
        balancer_min_liquidity,
        balancer_max_swap_fee,
    )
    .await;

    let pairs = build_pairs(uniswap_pairs, balancer_pools);
    log::info!("save | started");
    pairs.write().expect("saving failed");
}
