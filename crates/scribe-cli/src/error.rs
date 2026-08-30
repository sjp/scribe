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
pub fn exit_code(error: &anyhow::Error) -> ExitCode {
    if is_usage(error) {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Whether this error is a mistake in the request rather than a failure to
/// carry it out.
///
/// A renderer that rejects an option, or that is asked for an image it was
/// never given, was told to do something impossible by whoever wrote the
/// command line, so it counts as a mistake in the request even though it is
/// only noticed once the renderer sees it. That also makes it a mistake for
/// every other input of the same run, which is why a batch stops at the
/// first one rather than repeating it for each.
pub fn is_usage(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.is::<UsageError>()
            || matches!(
                cause.downcast_ref::<RenderError>(),
                Some(
                    RenderError::UnknownOption { .. }
                        | RenderError::InvalidOption { .. }
                        | RenderError::InvalidChoice { .. }
                        | RenderError::UnusableOption { .. }
                        | RenderError::MissingImage { .. }
                )
            )
    })
}

/// Writes an error and everything under it to standard error.
pub fn report(error: &anyhow::Error) {
    eprintln!("error: {error}");
    causes(error);
}

/// The same, for a failure over one input of several, which is named first so
/// that a run through a batch says which of them each failure was about.
///
/// Most of what can go wrong over an input names it already; a renderer's
/// complaint knows nothing of files and does not. The name goes on the front
/// of the ones that need it and no others.
pub fn report_about(input: &str, error: &anyhow::Error) {
    let message = error.to_string();
    if message.starts_with(input) {
        eprintln!("error: {message}");
    } else {
        eprintln!("error: {input}: {message}");
    }
    causes(error);
}

/// Writes everything under an error, one cause to a line.
fn causes(error: &anyhow::Error) {
    for cause in error.chain().skip(1) {
        eprintln!("  caused by: {cause}");
    }
}
