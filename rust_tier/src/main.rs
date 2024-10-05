use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::{noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Stream, StreamProtocol};
use libp2p_stream;
use std::{error::Error, time::Duration};
use tokio::io;

const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/stream");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let maybe_address = std::env::args()
        .nth(1)
        .map(|arg| arg.parse::<Multiaddr>())
        .transpose();

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| libp2p_stream::Behaviour::new())?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut income = swarm
        .behaviour()
        .new_control()
        .accept(STREAM_PROTOCOL)
        .unwrap();

    tokio::spawn(async move {
        while let Some((_, stream)) = income.next().await {
            match echo(stream).await {
                Ok(n) => {
                    println!("Received {n} bytes!");
                }
                Err(e) => {
                    println!("Failed in receive: {e}");
                }
            }
        }
    });

    if let Ok(Some(address)) = maybe_address {
        println!("Prepare to connect");

        swarm.dial(address)?;

        println!("Connect Done");
    }

    loop {
        let event = swarm.next().await.expect("never terminates");

        match event {
            SwarmEvent::Behaviour(e) => println!("Behaviour event => {:?}", e),
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                println!("Connection established => {peer_id} / {:?}", endpoint);
                tokio::spawn(handler(peer_id, swarm.behaviour().new_control()));
            }
            SwarmEvent::ConnectionClosed { peer_id, endpoint, .. } => println!("Connection closed => {peer_id} / {:?}", endpoint),
            SwarmEvent::NewListenAddr { address, .. } => println!("New addr connect => {address}"),
            _ => {}
        }
    }
}

async fn echo(mut stream: Stream) -> io::Result<usize> {
    let mut total = 0;

    let mut buf = [0u8; 100];

    loop {
        let read = stream.read(&mut buf).await?;
        if read == 0 {
            return Ok(total);
        }

        total += read;
        stream.write_all(&buf[..read]).await?;
    }
}

async fn handler(peer: PeerId, mut control: libp2p_stream::Control) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let stream = match control.open_stream(peer, STREAM_PROTOCOL).await {
            Ok(s) => s,
            Err(e @ libp2p_stream::OpenStreamError::UnsupportedProtocol(_)) => {
                println!("Peer do not support stream protocol: {:?}", e);
                return;
            }
            Err(e) => {
                println!("Error in handler: {:?}", e);
                return;
            }
        };

        if let Err(e) = send(stream).await {
            println!("Fail in send process: {:?}", e);
        }

        println!("Completed!!");
    }
}

async fn send(mut stream: Stream) -> io::Result<()> {
    let message = b"Hello Stream";

    stream.write_all(message).await?;

    let mut buf = vec![0; message.len()];
    stream.read_exact(&mut buf).await?;

    if message != buf.as_slice() {
        return Err(io::Error::new(io::ErrorKind::Other, "incorrect echo"));
    }

    stream.close().await?;

    Ok(())
}