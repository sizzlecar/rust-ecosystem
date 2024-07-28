use std::{net::SocketAddr, sync::Arc};

use dashmap::DashMap;
use futures::SinkExt;
use tokio::net::{
    tcp::{ReadHalf, WriteHalf},
    TcpListener,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tracing::info;
/// 客户端信息
#[derive(Debug)]
#[allow(dead_code)]
struct Client {
    name: String,
    addr: String,
    writer: Sender<String>,
}

impl Client {
    fn new(name: String, addr: String, writer: Sender<String>) -> Self {
        Self { name, addr, writer }
    }
}

#[derive(Debug)]
struct State {
    pub inner: DashMap<SocketAddr, Client>,
}

impl State {
    fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    fn insert(&self, addr: SocketAddr, client: Client) {
        self.inner.insert(addr, client);
    }

    fn remove(&self, addr: &SocketAddr) {
        self.inner.remove(addr);
    }
}

#[tokio::main]
#[allow(unreachable_code)]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let addr = "127.0.0.1:8888";
    let listener: TcpListener = TcpListener::bind(addr).await?;
    info!("Listening on: {}", addr);
    let state_arc = Arc::new(State::new());
    loop {
        let (mut stream, client_addr) = listener.accept().await?;
        let state_clone = state_arc.clone();
        let (tx, mut rx) = mpsc::channel::<String>(32);
        tokio::spawn(async move {
            info!("Accepted connection from: {}", client_addr);
            let (r, w) = stream.split();
            let mut framed_write: FramedWrite<tokio::net::tcp::WriteHalf, LinesCodec> =
                FramedWrite::new(w, LinesCodec::new());
            let mut framed_read: FramedRead<tokio::net::tcp::ReadHalf, LinesCodec> =
                FramedRead::new(r, LinesCodec::new());
            let current_name = ceare_client(
                &state_clone,
                client_addr,
                tx,
                &mut framed_read,
                &mut framed_write,
            )
            .await?;

            //持续接受消息并且广播
            loop {
                tokio::select! {
                    result = send_to_client(&mut rx, &mut framed_write) => {
                        match result {
                            Ok(true) => {}
                            Ok(false) => {
                                break;
                            }
                            Err(e) => {
                                info!("Error sending to client: {}", e);
                                break;
                            }
                        }
                    }
                    result = receive_and_broadcast(&mut framed_read, client_addr, &state_clone, current_name.as_str()) => {
                        match result {
                            Ok(true) => {}
                            Ok(false) => {
                                break;
                            }
                            Err(e) => {
                                info!("Error sending to client: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
            anyhow::Result::<bool>::Ok(true)
        });
    }
    Ok(())
}

async fn broadcast(
    state: &Arc<State>,
    name: &str,
    message: &str,
    addr: &SocketAddr,
    exclude_self: bool,
) -> anyhow::Result<()> {
    for item in state.as_ref().inner.iter() {
        let client = item.value();
        if exclude_self && client.addr == addr.to_string() {
            info!("exclude: {}", addr);
            continue;
        }
        info!("send: {} to {}", message, client.addr);
        match client.writer.send(format!("{}: {}", name, message)).await {
            Ok(_) => {}
            Err(e) => {
                info!("Error: {}", e);
            }
        };
    }

    Ok(())
}

async fn ceare_client<'a>(
    state: &Arc<State>,
    client_addr: SocketAddr,
    tx: Sender<String>,
    framed_read: &mut FramedRead<tokio::net::tcp::ReadHalf<'a>, LinesCodec>,
    framed_write: &mut FramedWrite<tokio::net::tcp::WriteHalf<'a>, LinesCodec>,
) -> anyhow::Result<String> {
    let _ = framed_write
        .send(String::from("Hello, what's your name?\n"))
        .await;
    let mut current_name = String::new();
    match framed_read.next().await {
        Some(Ok(line)) => {
            println!("Received line: {}", line);
            let line = line.clone();
            current_name.push_str(&line);
            let client = Client::new(line.clone(), client_addr.to_string(), tx);
            state.insert(client_addr, client);
            broadcast(
                state,
                "System",
                &format!("{} join", &line),
                &client_addr,
                false,
            )
            .await?;
        }
        Some(Err(e)) => {
            info!("Error: {}", e);
        }
        None => {
            info!("Connection closed");
        }
    };
    Ok(current_name)
}

async fn send_to_client(
    rx: &mut Receiver<String>,
    framed_write: &mut FramedWrite<WriteHalf<'_>, LinesCodec>,
) -> anyhow::Result<bool> {
    match rx.recv().await {
        Some(msg) => {
            info!("send: {}", msg);
            match framed_write.send(msg).await {
                Ok(_) => {}
                Err(e) => {
                    info!("Error: {}", e);
                }
            };
            Ok(true)
        }
        None => {
            info!("Connection closed");
            Ok(false)
        }
    }
}

async fn receive_and_broadcast(
    framed_read: &mut FramedRead<ReadHalf<'_>, LinesCodec>,
    client_addr: SocketAddr,
    state_clone: &Arc<State>,
    current_name: &str,
) -> anyhow::Result<bool> {
    match framed_read.next().await {
        Some(Ok(line)) => {
            info!("Received line: {} from: {}", line, client_addr);
            broadcast(state_clone, current_name, &line, &client_addr, true)
                .await
                .unwrap();
            Ok(true)
        }
        Some(Err(e)) => {
            info!("Error: {}", e);
            Ok(true)
        }
        None => {
            info!("Connection closed");
            state_clone.remove(&client_addr);
            let _ = broadcast(
                state_clone,
                "System",
                format!("{} leave", current_name).as_str(),
                &client_addr,
                true,
            )
            .await;
            Ok(false)
        }
    }
}
