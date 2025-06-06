use std::{
    net::{AddrParseError, IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

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
    address: Option<String>,

    /// The location of the data directory. By default
    /// this is the current working directory.
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// The port podsync listens on.
    #[arg(short, long, default_value_t = 80)]
    port: u16,

    /// Emit the podsync version
    #[arg(short, long)]
    version: bool,
}

impl Args {
    pub fn addr(&self) -> Result<SocketAddr, AddrParseError> {
        self.address
            .as_deref()
            .unwrap_or("127.0.0.1")
            .parse()
            .map(|addr: IpAddr| (addr, self.port).into())
    }

    pub fn secure(&self) -> bool {
        self.secure
    }

    pub fn show_version(&self) -> bool {
        self.version
    }

    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }
}
