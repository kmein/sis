use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use eyre::Result;

use crate::systemd::Backend;

/// k9s for systemd.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Opts {
    /// Talk to the user service manager instead of the system one.
    #[arg(long)]
    pub user: bool,

    /// Write tracing output to this file (set RUST_LOG to control verbosity).
    #[arg(long, value_name = "FILE")]
    pub log_file: Option<PathBuf>,

    /// Open this view instead of `units` (e.g. `timers`, `failed`).
    pub view: Option<String>,

    /// Print a resource as tab-separated text and exit (debugging aid).
    #[arg(long, hide = true, value_enum)]
    pub dump: Option<DumpKind>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DumpKind {
    Units,
    UnitFiles,
    Jobs,
}

pub async fn dump(backend: &Backend, kind: DumpKind) -> Result<()> {
    match kind {
        DumpKind::Units => {
            for u in backend.manager.list_units().await? {
                println!("{}\t{}\t{}\t{}\t{}\t{}", u.0, u.2, u.3, u.4, u.6, u.1);
            }
        }
        DumpKind::UnitFiles => {
            for (path, state) in backend.manager.list_unit_files().await? {
                println!("{path}\t{state}");
            }
        }
        DumpKind::Jobs => {
            for j in backend.manager.list_jobs().await? {
                println!("{}\t{}\t{}\t{}", j.0, j.1, j.2, j.3);
            }
        }
    }
    Ok(())
}
