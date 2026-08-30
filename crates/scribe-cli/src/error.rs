//! Telling a mistake in the request apart from a failure to carry it out.
//!
//! The two leave the process with different statuses, so a script can retry
//! one and not the other. Everything that reaches [`main`](crate::main) is an
//! [`anyhow::Error`]; this module is how the chain is asked which kind it is.

use std::fmt;
use std::process::ExitCode;

use scribe_core::render::RenderError;

/// Nothing was wrong with the machinery: the command asked for something it
/// could not have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// A mistake in the request, ready to be returned or handed to `bail!`.
pub fn usage(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UsageError(message.into()))
}

/// The status the process leaves with when this error ends the run.
///
/// A renderer that rejects an option was told to do something impossible by
/// whoever wrote the command line, so it counts as a mistake in the request
/// even though it is only noticed once the renderer sees it.
pub fn exit_code(error: &anyhow::Error) -> ExitCode {
    let misused = error.chain().any(|cause| {
        cause.is::<UsageError>()
            || matches!(
                cause.downcast_ref::<RenderError>(),
                Some(
                    RenderError::UnknownOption { .. }
                        | RenderError::InvalidOption { .. }
                        | RenderError::InvalidChoice { .. }
                        | RenderError::UnusableOption { .. }
                )
            )
    });
    if misused {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Writes an error and everything under it to standard error.
pub fn report(error: &anyhow::Error) {
    eprintln!("error: {error}");
    for cause in error.chain().skip(1) {
        eprintln!("  caused by: {cause}");
    }
}
