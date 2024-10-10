use crate::{Device, DeviceConfig};
use async_trait::async_trait;
use tun_rs::{AbstractDevice, AsyncDevice};

pub struct TunDevice {
    tun: AsyncDevice,
}

#[async_trait]
impl Device<tun_rs::Error> for TunDevice {
    fn new(config: DeviceConfig) -> Result<TunDevice, tun_rs::Error> {
        let mut tun_config = tun_rs::Configuration::default();

        for v4 in config.ipv4_addresses {
            tun_config.address_with_prefix(v4.ip(), v4.prefix());
        }

        tun_config.name(config.name);
        tun_config.mtu(config.mtu);

        let device = match tun_rs::create_as_async(&tun_config) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{:?}", e);
                return Err(e);
            }
        };

        for v6 in config.ipv6_addresses {
            device.add_address_v6(std::net::IpAddr::V6(v6.network()), v6.prefix())?;
        }

        Ok(TunDevice { tun: device })
    }

    async fn read(&mut self, buf: &mut [u8]) -> tokio::io::Result<usize> {
        return self.tun.recv(buf).await;
    }

    async fn write(&mut self, buf: &[u8]) -> tokio::io::Result<usize> {
        return self.tun.send(buf).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnetwork::Ipv4Network;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_tun_device() {
        let config = DeviceConfig {
            name: "tun0".to_string(),
            mtu: 1500,
            ipv4_addresses: vec![Ipv4Network::new(Ipv4Addr::new(10, 10, 10, 1), 24).unwrap()],
            ipv6_addresses: vec![],
        };

        let tun_device = TunDevice::new(config).expect("Failed to create TunDevice");

        assert_eq!(tun_device.tun.name().unwrap(), "tun0");
    }
}
