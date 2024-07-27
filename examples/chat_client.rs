use std::{error::Error, net::SocketAddr};

use futures::{future, SinkExt};
use tokio::{io, net::TcpStream};
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let addr = "127.0.0.1:8888";
    let stdin = FramedRead::new(io::stdin(), BytesCodec::new());
    let mut stdin = stdin.map(|i| i.map(|bytes| bytes.freeze()));
    let mut stdout = FramedWrite::new(io::stdout(), BytesCodec::new());
    let mut stream = TcpStream::connect(addr.parse::<SocketAddr>()?).await?;
    let (r, w) = stream.split();
    let mut sink = FramedWrite::new(w, BytesCodec::new());
    // filter map Result<BytesMut, Error> stream into just a Bytes stream to match stdout Sink
    // on the event of an Error, log the error and end the stream
    let mut stream = FramedRead::new(r, BytesCodec::new())
        .filter_map(|i| match i {
            //BytesMut into Bytes
            Ok(i) => Some(i.freeze()),
            Err(e) => {
                println!("failed to read from socket; error={}", e);
                None
            }
        })
        .map(Ok);

    let _ = future::join(sink.send_all(&mut stdin), stdout.send_all(&mut stream)).await;

    Ok(())
}
