use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "hfs",
    about = "Heavy / Honest File Storage — a high-performance Git LFS alternative",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize HFS in the current repository
    Init,

    /// Track file patterns (add to .gitattributes)
    Track {
        /// Glob patterns to track (e.g. "*.bin" "*.dat")
        patterns: Vec<String>,
    },

    /// Stop tracking file patterns
    Untrack {
        /// Glob patterns to untrack
        patterns: Vec<String>,
    },

    /// Show HFS status: store stats, tracked patterns, stored files
    Status,

    /// Garbage collect orphaned objects
    Gc {
        /// Show what would be removed without actually removing
        #[arg(long)]
        dry_run: bool,
    },

    /// Push chunks to remote storage
    Push,

    /// Pull chunks from remote storage
    Pull,

    /// Fetch all chunks for pointer files after git clone
    Clone,

    /// List HFS-tracked files
    LsFiles,

    /// Run as a long-running Git filter process (called by Git, not by users)
    FilterProcess,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    match cli.command {
        Commands::Init => hfs::cli::init::run(&cwd),

        Commands::Track { patterns } => hfs::cli::track::run(&cwd, &patterns),

        Commands::Untrack { patterns } => hfs::cli::untrack::run(&cwd, &patterns),

        Commands::Status => hfs::cli::status::run(&cwd),

        Commands::Gc { dry_run } => hfs::cli::gc::run(&cwd, dry_run),

        Commands::Push => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(hfs::cli::push::run(&cwd))
        }

        Commands::Pull => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(hfs::cli::pull::run(&cwd))
        }

        Commands::Clone => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(hfs::cli::clone::run(&cwd))
        }

        Commands::LsFiles => hfs::cli::ls_files::run(&cwd),

        Commands::FilterProcess => {
            let hfs_dir = find_hfs_dir(&cwd)?;
            let store = hfs::cas::Store::new(&hfs_dir);
            hfs::filter::process::run_filter_process(&store)
        }
    }
}

fn find_hfs_dir(cwd: &PathBuf) -> Result<PathBuf> {
    hfs::config::Config::find_hfs_dir(cwd).ok_or_else(|| {
        anyhow::anyhow!("not an HFS repository (no .hfs directory found)\nRun `hfs init` first.")
    })
}
