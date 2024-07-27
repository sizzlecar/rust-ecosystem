use std::{net::SocketAddr, sync::Arc};

use dashmap::DashMap;
use futures::SinkExt;
use tokio::sync::mpsc;
use tokio::{net::TcpListener, sync::mpsc::Sender};
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
    //全局状态
    let state_arc = Arc::new(State::new());

    loop {
        let (mut stream, client_addr) = listener.accept().await?;
        let state_clone = state_arc.clone();
        let (tx, mut rx) = mpsc::channel::<String>(32);
        tokio::spawn(async move {
            info!("Accepted connection from: {}", client_addr);
            let (r, w) = stream.split();
            let mut framed_write = FramedWrite::new(w, LinesCodec::new());
            let mut framed_read = FramedRead::new(r, LinesCodec::new());
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
                    state_clone.insert(client_addr, client);
                    broadcast(
                        &state_clone,
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

            //持续接受消息并且广播
            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(msg) => {
                                info!("send: {}", msg);
                                match framed_write.send(msg).await {
                                    Ok(_) => {}
                                    Err(e) => {
                                        info!("Error: {}", e);
                                    }
                                };
                            }
                            None => {
                                info!("Connection closed");
                                break;
                            }
                        }
                    }
                    line = framed_read.next() => {
                        info!("loop Received line start");
                        match line {
                            Some(Ok(line)) => {
                                info!("Received line: {} from: {}", line, client_addr);
                                broadcast(&state_clone, &current_name, &line, &client_addr, true).await?;
                            }
                            Some(Err(e)) => {
                                info!("Error: {}", e);
                            }
                            None => {
                                info!("Connection closed");
                                state_clone.remove(&client_addr);
                                broadcast(
                                    &state_clone,
                                    "System",
                                    format!("{} leave", &current_name).as_str(),
                                    &client_addr,
                                    true,
                                )
                                .await?;
                                break;
                            }
                        };
                    }
                }
                ()
            }

            anyhow::Result::<bool>::Ok(true)
        });

        ()
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
