use std::net::{ToSocketAddrs, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// Whether podsync's clients connect to it over https.
    /// If so, the sessionid cookie is sent as a secure cookie.
    #[arg(short, long)]
    secure: bool,

    /// Whether podsync listens on the wildcard address. By default
    /// podsync will listen just on the loopback.
    #[arg(short, long)]
    any_address: bool,

    /// The port podsync listens on.
    #[arg(short, long, default_value_t = 80)]
    port: u16,
}

impl Args {
    pub fn addrs(&self) -> Vec<SocketAddr> {
        let hosts = if self.any_address {
            [
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)),
            ]
        } else {
            [
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)),
            ]
        };

        hosts.into_iter().map(|addr| (addr, self.port).into()).collect::<Vec<SocketAddr>>()
    }

    pub fn secure(&self) -> bool {
        self.secure
    }
}
