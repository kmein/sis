mod cli;
mod systemd;

use std::io;

use clap::Parser;
use eyre::Result;
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::Opts;

fn main() -> Result<()> {
    color_eyre::install()?;
    let opts = Opts::parse();
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(opts))
}

async fn run(opts: Opts) -> Result<()> {
    let scope = if opts.user { systemd::Scope::User } else { systemd::Scope::System };
    let backend = systemd::Backend::connect(scope).await?;
    if let Some(kind) = opts.dump {
        return cli::dump(&backend, kind).await;
    }
    eyre::bail!("the TUI is not implemented yet; try --dump units")
}
