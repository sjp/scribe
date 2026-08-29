//! Command line entry point for scribe.
//!
//! Recognises the text in raster images and renders it into a format that
//! keeps it searchable and selectable. Every file, terminal and process
//! concern lives in this crate; `scribe-core` does the work and touches
//! nothing outside itself.

mod cli;
mod error;
mod io;
mod models;
mod run;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Logging};

fn main() -> ExitCode {
    let cli = Cli::parse();
    start_logging(&cli.logging);
    match run::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error::report(&error);
            error::exit_code(&error)
        }
    }
}

/// Sends the run's own account of itself to standard error.
///
/// `RUST_LOG` overrides the flags, so that a filter of any shape the log
/// crate understands is still available.
fn start_logging(logging: &Logging) {
    let level = if logging.quiet {
        log::LevelFilter::Error
    } else {
        match logging.verbose {
            0 => log::LevelFilter::Warn,
            1 => log::LevelFilter::Info,
            2 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        }
    };
    env_logger::Builder::new()
        .filter_level(level)
        .format_target(false)
        .format_timestamp(None)
        .parse_default_env()
        .init();
}
