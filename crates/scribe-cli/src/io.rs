//! Reading what a run is given and writing what it produces.
//!
//! This is the whole of scribe's contact with the filesystem and the
//! terminal; everything below it works in bytes.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

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

/// How a document written into `directory` points at `image`.
///
/// A link is resolved against the document it sits in, not against the
/// directory the run was started from, so a path typed on the command line
/// has to be spelled out again from where the output lands. Both are
/// anchored to the working directory and then compared as they are written,
/// without asking the filesystem what they lead to, and the answer comes back
/// with forward slashes because it is going into a URL.
///
/// This is `None` when the two share no root to count from, when either path
/// is not UTF-8, or when the working directory cannot be read; the caller
/// then has nothing better than the path as it was typed.
pub fn href_from(directory: &Path, image: &Path) -> Option<String> {
    let working = std::env::current_dir().ok()?;
    let (from, to) = (anchored(&working, directory), anchored(&working, image));
    let (directory, image) = (spelling(&from), spelling(&to));

    let shared = directory
        .iter()
        .zip(&image)
        .take_while(|(one, other)| one == other)
        .count();
    if shared == 0 || shared == image.len() {
        return None;
    }

    let mut href = "../".repeat(directory.len() - shared);
    for (position, part) in image[shared..].iter().enumerate() {
        if position > 0 {
            href.push('/');
        }
        href.push_str(part.as_os_str().to_str()?);
    }
    Some(href)
}

/// A path made absolute against the working directory, if it was not already.
fn anchored(working: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        working.join(path)
    }
}

/// The parts of a path as it is written, with `.` dropped and `..` taken to
/// mean the name before it.
///
/// This is what a link resolver does, and it is deliberately not what the
/// filesystem does: no symbolic link is followed and nothing has to exist.
fn spelling(path: &Path) -> Vec<Component<'_>> {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => {
                parts.pop();
            }
            component => parts.push(component),
        }
    }
    parts
}

/// Makes sure a directory named for outputs exists.
pub fn make_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("{} cannot be created", directory.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_below_the_output_is_reached_by_name() {
        assert_eq!(
            href_from(Path::new("/work/out"), Path::new("/work/out/pages/a.png")),
            Some("pages/a.png".to_string())
        );
    }

    #[test]
    fn an_image_beside_the_output_is_reached_by_climbing_out() {
        assert_eq!(
            href_from(Path::new("/work/out"), Path::new("/work/scans/a.png")),
            Some("../scans/a.png".to_string())
        );
    }

    #[test]
    fn an_image_in_the_output_directory_is_reached_by_its_name_alone() {
        assert_eq!(
            href_from(Path::new("/work/out"), Path::new("/work/out/a.png")),
            Some("a.png".to_string())
        );
    }

    #[test]
    fn a_path_is_read_as_it_is_written_rather_than_followed() {
        assert_eq!(
            href_from(
                Path::new("/work/./out/deep/.."),
                Path::new("/work/scans/../a.png")
            ),
            Some("../a.png".to_string())
        );
    }

    #[test]
    fn an_image_that_is_the_output_directory_itself_has_no_href() {
        assert_eq!(
            href_from(Path::new("/work/out"), Path::new("/work/out")),
            None
        );
    }

    #[test]
    fn a_relative_pair_is_anchored_to_the_same_working_directory() {
        assert_eq!(
            href_from(Path::new("out"), Path::new("scans/a.png")),
            Some("../scans/a.png".to_string())
        );
    }
}
