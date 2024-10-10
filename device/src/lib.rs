use std::error::Error;

use async_trait::async_trait;
use ipnetwork::{Ipv4Network, Ipv6Network};

mod tun;

pub struct DeviceConfig {
    pub name: String,
    pub ipv4_addresses: Vec<Ipv4Network>,
    pub ipv6_addresses: Vec<Ipv6Network>,
    pub mtu: u16,
}

#[async_trait]
pub trait Device<E: Error>: Sized {
    fn new(config: DeviceConfig) -> Result<Self, E>
    where
        Self: Sized;

    async fn read(&mut self, buf: &mut [u8]) -> tokio::io::Result<usize>;
    async fn write(&mut self, buf: &[u8]) -> tokio::io::Result<usize>;
}
