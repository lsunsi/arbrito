mod gen;
mod resolve_pools;
mod watch_pools;

#[tokio::main]
async fn main() {
    let web_endpoint = std::env::var("WEB3_ENDPOINT")
        .map_err(|_| "Missing WEB3_ENDPOINT")
        .unwrap();

    watch_pools::watch_pools(web_endpoint, resolve_pools::resolve_pools().await.unwrap())
        .await
        .unwrap();
}
