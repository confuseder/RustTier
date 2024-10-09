use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::{noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Stream, StreamProtocol};
use libp2p_stream::{self, IncomingStreams};
use pnet::packet::{
    icmp::{checksum, IcmpPacket, IcmpTypes, MutableIcmpPacket},
    ipv4::{Ipv4Packet, MutableIpv4Packet},
    Packet,
};
use std::{
    error::Error,
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
    u8, vec,
};
use tokio::io;
use tun_rs::AsyncDevice;

const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/stream");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let maybe_address = std::env::args()
        .nth(1)
        .map(|arg| arg.parse::<Multiaddr>())
        .transpose();

    let ip_pr = std::env::args()
        .nth(2)
        .map(|arg| arg.parse::<u8>())
        .transpose()
        .expect("Failed to parse IP part");

    let ip_pr = ip_pr.unwrap_or(11);

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

    let mut tun_configuration = tun_rs::Configuration::default();
    tun_configuration
        .address_with_prefix((192, 168, 2, ip_pr), 24u8)
        .up();

    let tun_device = Arc::new(tun_rs::create_as_async(&tun_configuration)?);

    let income = swarm
        .behaviour()
        .new_control()
        .accept(STREAM_PROTOCOL)
        .unwrap();

    tokio::spawn(listener(
        income,
        tun_device.clone(),
        swarm.behaviour().new_control(),
    ));

    if let Ok(Some(address)) = maybe_address {
        println!("Prepare to connect");

        swarm.dial(address)?;

        println!("Connect Done");
    }

    loop {
        let event = swarm.next().await.expect("never terminates");

        match event {
            SwarmEvent::Behaviour(e) => println!("Behaviour event => {:?}", e),
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                println!("Connection established => {peer_id} / {:?}", endpoint);
                tokio::spawn(handler(
                    peer_id,
                    swarm.behaviour().new_control(),
                    tun_device.clone(),
                ));
            }
            SwarmEvent::ConnectionClosed {
                peer_id, endpoint, ..
            } => println!("Connection closed => {peer_id} / {:?}", endpoint),
            SwarmEvent::NewListenAddr { address, .. } => println!("New addr connect => {address}"),
            _ => {}
        }
    }
}

async fn listener(
    mut income: IncomingStreams,
    device: Arc<AsyncDevice>,
    control: libp2p_stream::Control,
) {
    while let Some((peer_id, stream)) = income.next().await {
        match recv(stream, device.clone(), peer_id, control.clone()).await {
            Ok(n) => {
                println!("Writed {n} bytes!");
            }
            Err(e) => {
                println!("Failed in receive: {e}");
            }
        }
    }
}

async fn recv(
    mut stream: Stream,
    device: Arc<AsyncDevice>,
    peer_id: PeerId,
    control: libp2p_stream::Control,
) -> io::Result<usize> {
    let mut total = 0;

    let mut buf = [0u8; 1500];
    let mut pack: Vec<u8> = Vec::new();

    loop {
        let read = stream.read(&mut buf).await?;
        if read == 0 {
            if let Some(ipv4_pack) = Ipv4Packet::new(&pack[..total]) {
                if ipv4_pack.get_destination() != Ipv4Addr::new(192, 168, 2, 3) {
                    return Ok(total);
                }

                if let Some(icmp_pack) = IcmpPacket::new(ipv4_pack.payload()) {
                    match icmp_pack.get_icmp_type() {
                        IcmpTypes::EchoRequest => {
                            println!(
                                "这是一个 ICMP Echo 请求包; now time => {}",
                                current_timestamp_millis()
                            );

                            let mut tmp_pack: Vec<u8> = ipv4_pack.packet().to_vec();
                            let mut tmp_res: Vec<u8> = icmp_pack.packet().to_vec();

                            let mut response_pack: MutableIpv4Packet =
                                MutableIpv4Packet::new(&mut tmp_pack)
                                    .expect("Failed to create MutableIpv4Packet");
                            let mut response_icmp: MutableIcmpPacket =
                                MutableIcmpPacket::new(&mut tmp_res)
                                    .expect("Failed to create MutableICMPPacket");

                            response_icmp.set_icmp_type(IcmpTypes::EchoReply);
                            response_icmp.set_checksum(checksum(&response_icmp.to_immutable()));

                            response_pack.set_payload(response_icmp.packet());

                            response_pack.set_source(ipv4_pack.get_destination());
                            response_pack.set_destination(ipv4_pack.get_source());

                            send_by_id(peer_id, response_pack.packet(), control).await;
                            println!("响应发送完毕 now time => {}", current_timestamp_millis());
                        }
                        IcmpTypes::EchoReply => {
                            println!("这是一个 ICMP Echo 响应包");
                        }
                        _ => {
                            println!("这是其他类型的 ICMP 包");
                        }
                    };
                }
            }

            return Ok(total);
        }

        total += read;
        pack.extend_from_slice(&buf[..read]);
        device.send(&buf[..read]).await?;
        stream.write_all(&buf[..read]).await?;
    }
}

async fn send(stream: &mut Stream, message: &[u8]) -> io::Result<()> {
    stream.write_all(message).await?;
    Ok(())
}

async fn send_by_id(peer_id: PeerId, message: &[u8], mut control: libp2p_stream::Control) {
    let mut stream = match control.open_stream(peer_id, STREAM_PROTOCOL).await {
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

    match send(&mut stream, message).await {
        Ok(()) => println!("Send to peer by id!"),
        Err(e) => println!("Error when send to peer => {e}"),
    };
}

async fn handler(peer: PeerId, mut control: libp2p_stream::Control, device: Arc<AsyncDevice>) {
    let mut buffer = vec![0; 1500];

    let mut stream = match control.open_stream(peer, STREAM_PROTOCOL).await {
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

    loop {
        let len = match device.recv(&mut buffer).await {
            Ok(len) => {
                println!(
                    "Received data from self tun => {len} bytes; now time => {}",
                    current_timestamp_millis()
                );
                len
            }
            Err(_) => {
                eprintln!("Error receiving data");
                break;
            }
        };

        if let Err(e) = send(&mut stream, &buffer[..len]).await {
            println!("Fail in send process: {:?}", e);
        }

        println!("Completed!! now time => {}", current_timestamp_millis());
    }
}

fn current_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
}
