use ethcontract::{BlockId, BlockNumber, Web3};
use futures::{Future, FutureExt, Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
use web3::{
    transports::WebSocket,
    types::{H160, U256, U64},
};

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub id: BlockId,
    pub number: U64,
    pub gas_price: U256,
    pub balance: U256,
    pub nonce: U256,
}

impl Block {
    async fn fetch(web3: Web3<WebSocket>, addr: H160, number: U64) -> Block {
        let block_number = BlockNumber::Number(number);

        let eth = web3.eth();
        let (nonce, balance, gas_price) = tokio::join!(
            eth.transaction_count(addr, Some(block_number)),
            eth.balance(addr, Some(block_number)),
            eth.gas_price(),
        );

        Block {
            id: BlockId::Number(block_number),
            gas_price: gas_price.expect("failed fetching block gas_price"),
            balance: balance.expect("failed fetching block balance"),
            nonce: nonce.expect("failed fetching block nonce"),
            number,
        }
    }
}

pub struct LatestBlock {
    requests_tx: mpsc::UnboundedSender<oneshot::Sender<Block>>,
    request_rx: Option<oneshot::Receiver<Block>>,
}

impl LatestBlock {
    pub fn new(web3: Web3<WebSocket>, executor_address: H160) -> LatestBlock {
        let (requests_tx, requests_rx) = mpsc::unbounded_channel();
        tokio::spawn(task(web3, executor_address, requests_rx));

        LatestBlock {
            request_rx: None,
            requests_tx,
        }
    }
}

impl Stream for LatestBlock {
    type Item = Block;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut self.request_rx {
            None => {
                let (request_tx, request_rx) = oneshot::channel();
                self.requests_tx.send(request_tx).expect("failed request");
                self.request_rx = Some(request_rx);
                Poll::Pending
            }
            Some(request_rx) => {
                let request_rx = Pin::new(request_rx);
                Future::poll(request_rx, cx).map(|res| {
                    self.request_rx = None;
                    Some(res.expect("tx was dropped"))
                })
            }
        }
    }
}

async fn task(
    web3: Web3<WebSocket>,
    executor_address: H160,
    mut requests_rx: mpsc::UnboundedReceiver<oneshot::Sender<Block>>,
) {
    let mut request = None;
    let mut open = true;

    let mut stream = web3
        .eth_subscribe()
        .subscribe_new_heads()
        .await
        .expect("failed subscribing to new heads");

    while open || request.is_some() {
        tokio::select! {
            tx = requests_rx.recv() => match tx {
                Some(tx) => request = Some(tx),
                None => open = false,
            },
            header = stream.next() => match header {
                None => break,
                Some(head) => {
                    if let Some(tx) = request.take() {
                        let head = head.expect("error reading new block head");
                        let number = head.number.expect("block without a number?");
                        let block = Block::fetch(web3.clone(), executor_address, number);
                        tokio::spawn(block.map(move |b| tx.send(b).expect("failed response")));
                    }
                }
            }
        }
    }
}
