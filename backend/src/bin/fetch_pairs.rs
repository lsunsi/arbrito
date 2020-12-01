use bigdecimal::{BigDecimal, BigDecimal as BigInt, ToPrimitive};
use futures::{Future, TryFutureExt};
use graphql_client::{GraphQLQuery, Response};
use pooller::{Pair, Pairs, Token};
use reqwest::Client;
use std::{collections::HashMap, collections::HashSet, fmt::Debug, str::FromStr, time::Duration};
use tokio::time::delay_for;
use web3::types::H160;

const UNISWAP_URL: &str = "https://api.thegraph.com/subgraphs/name/ianlapham/uniswapv2";
const BALANCER_URL: &str = "https://api.thegraph.com/subgraphs/name/balancer-labs/balancer-beta";
const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

const ALLOWED_TOKENS: [&str; 21] = [
    "C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", // WETH
    "514910771AF9Ca656af840dff83E8264EcF986CA", // LINK
    "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", // USDC
    "2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", // WBTC
    "6B175474E89094C44Da98b954EedeAC495271d0F", // DAI
    "5864c777697Bf9881220328BF2f16908c9aFCD7e", // BUSD
    "1f9840a85d5aF5bf1D1762F925BDADdC4201F984", // UNI
    "EA86074fdAC85E6a605cd418668C63d2716cdfBc", // AAVE
    "C011a73ee8576Fb46F5E1c5751cA3B9Fe0af2a6F", // SNX
    "0bc529c00C6401aEF6D220BE8C6Ea1667F6Ad93e", // YFI
    "04Fa0d235C4abf4BcF4787aF4CF447DE572eF828", // UMA
    "c00e94Cb662C3520282E6f5717214004A7f26888", // COMP
    "9f8F72aA9304c8B593d555F12eF6589cC3A579A2", // MKR
    "ba100000625a3754423978a60c9317c58a424e3D", // BAL
    "0F5D2fB29fb7d3CFeE444a200298f468908cC942", // MANA
    "dd974D5C2e2928deA5F71b9825b8b646686BD200", // KNC
    "967da4048cD07aB37855c090aAF366e4ce1b9F48", // OCEAN
    "0000000000085d4780B73119b644AE5ecd22b376", // TUSD
    "408e41876cCCDC0F92210600ef50372656052a38", // REN
    "BBbbCA6A901c926F240b89EacB641d8Aec7AEafD", // LRC
    "6B3595068778DD592e39A122f4f5a5cF09C90fE2", // SUSHI
];

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

fn parse_address(addr: &str) -> H160 {
    H160::from_str(addr.strip_prefix("0x").expect("missing prefix")).expect("h160 parsing failed")
}

fn parse_decimals(bigdec: &BigDecimal) -> usize {
    bigdec.to_usize().expect("decimals parsing failed")
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

async fn uniswap_pairs(
    client: &Client,
    weth_address: H160,
    allowed_tokens: &[H160],
) -> Vec<(H160, Token, Token)> {
    let tokens: Vec<_> = allowed_tokens
        .iter()
        .map(|addr| format!("{:?}", addr))
        .collect();

    let mut pairs0 = vec![];
    let mut pairs1 = vec![];

    for page in 0.. {
        log::info!("uniswap_pairs | started page {}", page + 1,);

        let query = UniswapGetPairs::build_query(uniswap_get_pairs::Variables {
            tokens: tokens.clone(),
            skip: 1000 * page,
        });

        let data: uniswap_get_pairs::ResponseData = send(&|| {
            client
                .post(UNISWAP_URL)
                .json(&query)
                .send()
                .and_then(|a| a.json())
        })
        .await;

        if data.pairs0.len() == 0 && data.pairs1.len() == 0 {
            break;
        }

        pairs0.extend(data.pairs0);
        pairs1.extend(data.pairs1);
    }

    log::debug!(
        "uniswap_pairs | counts pairs0={} pairs1={}",
        pairs0.len(),
        pairs1.len(),
    );

    let mut raw_pairs = vec![];
    let pairs1: HashSet<_> = pairs1.into_iter().map(|a| a.id).collect();

    for pair0 in pairs0 {
        if pairs1.contains(&pair0.id) {
            raw_pairs.push((
                parse_address(&pair0.id),
                (
                    pair0.token0.symbol.clone(),
                    parse_address(&pair0.token0.id),
                    parse_decimals(&pair0.token0.decimals),
                ),
                (
                    pair0.token1.symbol.clone(),
                    parse_address(&pair0.token1.id),
                    parse_decimals(&pair0.token1.decimals),
                ),
            ));
        }
    }

    log::info!("uniswap_pairs | total raw pairs {}", raw_pairs.len());

    let weth_pairs: HashMap<H160, H160> = raw_pairs
        .iter()
        .filter_map(|(address, raw_token0, raw_token1)| {
            if raw_token0.1 == weth_address {
                Some((raw_token1.1, *address))
            } else if raw_token1.1 == weth_address {
                Some((raw_token0.1, *address))
            } else {
                None
            }
        })
        .collect();

    let mut pairs = vec![];
    let mut dropped = 0;

    for (address, raw_token0, raw_token1) in raw_pairs {
        let token0 = {
            if raw_token0.1 == weth_address {
                Token {
                    symbol: raw_token0.0,
                    address: raw_token0.1,
                    decimals: raw_token0.2,
                    weth_uniswap_pair: None,
                }
            } else {
                let weth_uniswap_pair = match weth_pairs.get(&raw_token0.1) {
                    Some(a) => a,
                    None => {
                        dropped += 1;
                        continue;
                    }
                };

                Token {
                    symbol: raw_token0.0,
                    address: raw_token0.1,
                    decimals: raw_token0.2,
                    weth_uniswap_pair: Some(*weth_uniswap_pair),
                }
            }
        };

        let token1 = {
            if raw_token1.1 == weth_address {
                Token {
                    symbol: raw_token1.0,
                    address: raw_token1.1,
                    decimals: raw_token1.2,
                    weth_uniswap_pair: None,
                }
            } else {
                let weth_uniswap_pair = match weth_pairs.get(&raw_token1.1) {
                    Some(a) => a,
                    None => {
                        dropped += 1;
                        continue;
                    }
                };

                Token {
                    symbol: raw_token1.0,
                    address: raw_token1.1,
                    decimals: raw_token1.2,
                    weth_uniswap_pair: Some(*weth_uniswap_pair),
                }
            }
        };

        pairs.push((address, token0, token1));
    }

    log::info!(
        "uniswap_pairs | {} pairs fetched ({} dropped)",
        pairs.len(),
        dropped
    );
    pairs
}

async fn balancer_pools(client: &Client, uniswap_pairs: &[(H160, Token, Token)]) -> Vec<Vec<H160>> {
    let mut pools = vec![];
    let mut count = 0;

    for (index, (_, token0, token1)) in uniswap_pairs.into_iter().enumerate() {
        let query = BalancerGetPools::build_query(balancer_get_pools::Variables {
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

        let mut valid_pools = vec![];

        for pool in data.pools {
            let tokens = match pool.tokens {
                None => continue,
                Some(a) => a,
            };

            let t0 = tokens
                .iter()
                .find(|t| parse_address(&t.address) == token0.address)
                .expect("balancer_pools: Could not find token0 on pool");

            let t1 = tokens
                .iter()
                .find(|t| parse_address(&t.address) == token1.address)
                .expect("balancer_pools: Could not find token1 on pool");

            if t0.denorm_weight != t1.denorm_weight {
                log::debug!(
                    "balancer_pools: dropping pool for {} {} due to unbalanced weights {} {} ({})",
                    token0.symbol,
                    token1.symbol,
                    t0.denorm_weight,
                    t1.denorm_weight,
                    pool.id
                );
                continue;
            }

            valid_pools.push(parse_address(&pool.id));
        }

        log::info!(
            "balancer_pools | {:>3} / {:<3} | {:<6} {:>6} | {} pools fetched",
            index + 1,
            uniswap_pairs.len(),
            token0.symbol,
            token1.symbol,
            valid_pools.len()
        );

        count += valid_pools.len();
        pools.push(valid_pools);
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

    let weth_address = H160::from_str(WETH_ADDRESS).expect("weth address parsing failed");
    let allowed_tokens = ALLOWED_TOKENS
        .iter()
        .copied()
        .map(H160::from_str)
        .collect::<Result<Vec<_>, _>>()
        .expect("allowed tokens parsing failed");

    let client = reqwest::Client::new();

    let uniswap_pairs = uniswap_pairs(&client, weth_address, &allowed_tokens).await;
    let balancer_pools = balancer_pools(&client, &uniswap_pairs).await;

    let pairs = build_pairs(uniswap_pairs, balancer_pools);
    log::info!("save | started");
    pairs.write().expect("saving failed");
}
