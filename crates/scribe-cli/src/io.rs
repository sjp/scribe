//! Reading what a run is given and writing what it produces.
//!
//! This is the whole of scribe's contact with the filesystem and the
//! terminal; everything below it works in bytes.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::STREAM;

/// The name an output takes from an input read from standard input.
const STREAM_NAME: &str = "stdin";

/// The name an output takes from an input whose path has no name of its own.
const UNNAMED: &str = "image";

/// What a layout written alongside an output is called, after the stem it
/// shares with that output.
const LAYOUT_SUFFIX: &str = "layout.json";

/// What that name leaves on the end of the layout's own stem.
const LAYOUT_STEM_SUFFIX: &str = ".layout";

/// Whether a path means standard input or standard output rather than a file.
pub fn is_stream(path: &Path) -> bool {
    path.as_os_str() == STREAM
}

/// How a path reads in a message.
pub fn shown(path: &Path) -> String {
    if is_stream(path) {
        "standard input".to_string()
    } else {
        path.display().to_string()
    }
}

/// The name an output takes from this input, without any extension.
pub fn stem(path: &Path) -> Cow<'_, str> {
    if is_stream(path) {
        return Cow::Borrowed(STREAM_NAME);
    }
    match path.file_stem() {
        Some(stem) => stem.to_string_lossy(),
        None => Cow::Borrowed(UNNAMED),
    }
}

/// The name an output takes from a layout document.
///
/// A layout written beside an output is named after the stem it shares with
/// that output, so the `.layout` comes off again here: rendering
/// `page.layout.json` writes `page.svg` rather than `page.layout.svg`, and
/// the result lands beside the image both of them came from.
pub fn layout_stem(path: &Path) -> String {
    let stem = stem(path);
    stem.strip_suffix(LAYOUT_STEM_SUFFIX)
        .unwrap_or(&stem)
        .to_string()
}

/// Reads a file, or all of standard input for `-`.
pub fn read(path: &Path) -> Result<Vec<u8>> {
    if is_stream(path) {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .read_to_end(&mut bytes)
            .context("standard input cannot be read")?;
        return Ok(bytes);
    }
    fs::read(path).with_context(|| format!("{} cannot be read", path.display()))
}

/// Reads a file as UTF-8 text, or all of standard input for `-`.
pub fn read_text(path: &Path) -> Result<String> {
    String::from_utf8(read(path)?).with_context(|| format!("{} is not UTF-8 text", shown(path)))
}

/// Writes to standard output, treating a closed pipe as a normal end.
///
/// A run whose output is piped into something that stops reading — `head`,
/// say — has already done what was asked of it, and should not end in a
/// panic for it.
pub fn print(bytes: &[u8]) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let written = stdout.write_all(bytes).and_then(|()| stdout.flush());
    match written {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result.context("standard output cannot be written"),
    }
}

/// Where one rendered document goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    /// Standard output, which is where a single result goes by default.
    Stdout,
    /// A file, named outright or built from an input's name.
    File(PathBuf),
}

impl Destination {
    /// Writes a document, replacing whatever was there before.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Stdout => print(bytes),
            Self::File(path) => fs::write(path, bytes)
                .with_context(|| format!("{} cannot be written", path.display())),
        }
    }

    /// How this destination reads in a message.
    pub fn shown(&self) -> String {
        match self {
            Self::Stdout => "standard output".to_string(),
            Self::File(path) => path.display().to_string(),
        }
    }

    /// Where a layout goes when it is asked for without a path of its own:
    /// beside the document it was rendered into, or beside the image when
    /// that document went to standard output.
    ///
    /// This is `None` when neither exists, which is a run reading standard
    /// input and writing standard output.
    pub fn layout_beside(&self, input: &Path) -> Option<PathBuf> {
        let anchor = match self {
            Self::File(path) => path,
            Self::Stdout if !is_stream(input) => input,
            Self::Stdout => return None,
        };
        Some(anchor.with_file_name(format!("{}.{LAYOUT_SUFFIX}", stem(anchor))))
    }
}

/// Makes sure a directory named for outputs exists.
pub fn make_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("{} cannot be created", directory.display()))
}
