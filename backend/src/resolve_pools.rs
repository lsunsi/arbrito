use bigdecimal::{BigDecimal, FromPrimitive};
use futures::future::join_all;
use graphql_client::{GraphQLQuery, Response};
use reqwest::Client;

const UNISWAP_URL: &str = "https://api.thegraph.com/subgraphs/name/ianlapham/uniswapv2";
const BALANCER_URL: &str = "https://api.thegraph.com/subgraphs/name/balancer-labs/balancer-beta";
const WETH_ADDRESS: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

const UNISWAP_MIN_ETH_RESERVE: i64 = 30_000;
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

#[derive(Clone)]
pub struct Token {
    pub address: String,
    pub name: String,
}

pub struct BalancerPool {
    pub address: String,
    pub token: Token,
}

#[derive(Debug)]
pub enum Error {
    Network(reqwest::Error),
    Graphql(Vec<graphql_client::Error>),
}

async fn uniswap_tokens(client: &Client) -> Result<Vec<Token>, Error> {
    let query = UniswapGetPairs::build_query(uniswap_get_pairs::Variables {
        min_reserve_eth: UNISWAP_MIN_ETH_RESERVE.into(),
        weth_address: String::from(WETH_ADDRESS),
    });

    let response = client
        .post(UNISWAP_URL)
        .json(&query)
        .send()
        .await
        .map_err(Error::Network)?;

    let body: Response<uniswap_get_pairs::ResponseData> =
        response.json().await.map_err(Error::Network)?;

    if let Some(errors) = body.errors {
        return Err(Error::Graphql(errors));
    }

    let mut tokens = vec![];

    if let Some(data) = body.data {
        for pair in data.pairs0 {
            tokens.push(Token {
                address: pair.token.id,
                name: pair.token.name,
            });
        }

        for pair in data.pairs1 {
            tokens.push(Token {
                address: pair.token.id,
                name: pair.token.name,
            });
        }
    }

    Ok(tokens)
}

async fn balancer_pools(client: &Client, token: Token) -> Result<Vec<BalancerPool>, Error> {
    let query = BalancerGetPools::build_query(balancer_get_pools::Variables {
        min_liquidity: BigDecimal::from_u64(BALANCER_MIN_LIQUIDITY).unwrap(),
        max_swap_fee: BigDecimal::from_f64(BALANCER_MAX_SWAP_FEE).unwrap(),
        tokens: vec![String::from(WETH_ADDRESS), token.address.clone()],
    });

    let res = client
        .post(BALANCER_URL)
        .json(&query)
        .send()
        .await
        .map_err(Error::Network)?;

    let body: graphql_client::Response<balancer_get_pools::ResponseData> =
        res.json().await.map_err(Error::Network)?;

    if let Some(errors) = body.errors {
        return Err(Error::Graphql(errors));
    }

    let mut pools = vec![];

    if let Some(data) = body.data {
        for pool in data.pools {
            pools.push(BalancerPool {
                token: token.clone(),
                address: pool.id,
            })
        }
    }

    Ok(pools)
}

pub async fn resolve_pools() -> Result<Vec<BalancerPool>, Error> {
    let client = reqwest::Client::new();
    let uniswap_tokens = uniswap_tokens(&client).await?;

    let mut balancer_pools_futures = vec![];
    for token in uniswap_tokens {
        balancer_pools_futures.push(balancer_pools(&client, token));
    }

    let mut balancer_pools = vec![];
    for balancer_pool_result in join_all(balancer_pools_futures).await {
        balancer_pools.extend(balancer_pool_result?);
    }

    Ok(balancer_pools)
}
