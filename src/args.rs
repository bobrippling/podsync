use std::net::{AddrParseError, IpAddr, SocketAddr};

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    /// Whether podsync's clients connect to it over https.
    /// If so, the sessionid cookie is sent as a secure cookie.
    #[arg(short, long)]
    secure: bool,

    /// The address podsync should listen on. By default
    /// podsync will listen just on the IPv4 loopback.
    #[arg(short, long)]
    address: String,

    /// The port podsync listens on.
    #[arg(short, long, default_value_t = 80)]
    port: u16,
}

impl Args {
    pub fn addr(&self) -> Result<SocketAddr, AddrParseError> {
        self.address
            .parse()
            .map(|addr: IpAddr| (addr, self.port).into())
    }

    pub fn secure(&self) -> bool {
        self.secure
    }
}
