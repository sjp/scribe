//! Command line entry point for scribe.

use clap::Parser;

/// Make the text in raster images searchable and selectable.
#[derive(Debug, Parser)]
#[command(name = "scribe", version)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let Cli {} = Cli::parse();
    Ok(())
}
