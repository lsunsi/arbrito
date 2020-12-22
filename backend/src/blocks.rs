use ethcontract::{BlockId, BlockNumber, Web3};
use futures::{Stream, StreamExt};
use tokio::sync::{mpsc, oneshot};
use web3::{
    transports::WebSocket,
    types::{BlockHeader, H160, U256, U64},
};

#[derive(Clone, Copy)]
pub struct Block {
    pub id: BlockId,
    pub number: U64,
    pub gas_price: U256,
    pub balance: U256,
    pub nonce: U256,
}

impl Block {
    async fn fetch(web3: &Web3<WebSocket>, addr: H160, number: U64) -> Block {
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

pub struct Blocks<'a> {
    executor_address: H160,
    web3: &'a Web3<WebSocket>,
    tx: mpsc::UnboundedSender<oneshot::Sender<BlockHeader>>,
}

impl<'a> Blocks<'a> {
    pub async fn new(web3: &'a Web3<WebSocket>, executor_address: H160) -> Blocks<'a> {
        let stream = web3.eth_subscribe().subscribe_new_heads();
        let stream = stream.await.expect("failed subscribing to new heads");

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(task(stream, rx));

        Blocks {
            executor_address,
            web3,
            tx,
        }
    }

    pub async fn latest(&self) -> Block {
        let (tx, rx) = oneshot::channel();
        self.tx.send(tx).expect("task could not receive request");
        let head = rx.await.expect("latest block tx was dropped");
        let number = head.number.expect("block head without a number?");
        Block::fetch(self.web3, self.executor_address, number).await
    }
}

async fn task(
    mut stream: impl Stream<Item = Result<BlockHeader, web3::Error>> + Unpin,
    mut rx: mpsc::UnboundedReceiver<oneshot::Sender<BlockHeader>>,
) {
    let mut latest = None;
    let mut open = true;

    while open && latest.is_some() {
        tokio::select! {
            tx = rx.recv() => match tx {
                Some(tx) => latest = Some(tx),
                None => open = false,
            },
            header = stream.next() => match header {
                None => break,
                Some(head) => {
                    if let Some(tx) = latest.take() {
                        let head = head.expect("error reading new block head");
                        tx.send(head).expect("latest block rx was dropped");
                    }
                }
            }
        }
    }
}
