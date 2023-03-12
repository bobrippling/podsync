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
    pub fn addr(&self) -> impl Into<std::net::SocketAddr> {
        if self.any_address {
            ([0, 0, 0, 0], self.port)
        } else {
            ([127, 0, 0, 1], self.port)
        }
    }

    pub fn secure(&self) -> bool {
        self.secure
    }
}
