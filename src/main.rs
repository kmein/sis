mod app;
mod cli;
mod event;
mod keys;
mod resources;
mod runtime;
mod store;
mod systemd;
mod ui;

use std::{fs::File, sync::Mutex};

use clap::Parser;
use eyre::{Context, Result};
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::Opts;

fn main() -> Result<()> {
    color_eyre::install()?;
    let opts = Opts::parse();
    if let Some(path) = &opts.log_file {
        let file = File::create(path).with_context(|| format!("creating log file {}", path.display()))?;
        fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .init();
    }

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

    // ratatui's panic hook restores the terminal before color-eyre reports.
    let mut terminal = ratatui::init();
    let result = runtime::Runtime::new(backend, opts.view).run(&mut terminal).await;
    ratatui::restore();
    result
}
