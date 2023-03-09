use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long)]
    secure: bool,

    #[arg(short, long)]
    local: bool,

    #[arg(short, long, default_value_t = 80)]
    port: u16,
}

impl Args {
    pub fn addr(&self) -> impl Into<std::net::SocketAddr> {
        if self.local {
            ([127, 0, 0, 1], self.port)
        } else {
            ([0, 0, 0, 0], self.port)
        }
    }

    pub fn secure(&self) -> bool {
        self.secure
    }
}
