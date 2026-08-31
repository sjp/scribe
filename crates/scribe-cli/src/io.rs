//! Reading what a run is given and writing what it produces.
//!
//! This is the whole of scribe's contact with the filesystem and the
//! terminal; everything below it works in bytes.

use std::borrow::Cow;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// What a document is called while it is still being written, before and
/// after the name of the file it is going to become. A leading dot keeps it
/// out of the way, and the tail is nothing a renderer would ever produce, so
/// a file caught mid-write is not mistaken for an output.
const TEMPORARY: (&str, &str) = (".", ".tmp-");

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
    ///
    /// A file arrives whole or not at all. Anything that goes wrong part way
    /// through — a full disk, an interrupted run — leaves whatever was at the
    /// path before still there, rather than half of what was on its way.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Stdout => print(bytes),
            Self::File(path) => write_whole(path, bytes)
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

/// Writes a file by writing another beside it and renaming that one over it.
///
/// The rename is the step that replaces the file, and it is one step: a
/// reader of the path sees either what was there before or the whole of what
/// is now there. Everything that can fail happens before it, and what has
/// been written so far is taken away again when it does.
///
/// A rename is only that within one filesystem, which is why the file being
/// written is a sibling of its target rather than something in the system's
/// temporary directory.
fn write_whole(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = beside(path)?;
    let written = fill(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Writes the bytes of one file and waits for them to reach the disk, so that
/// the rename that follows cannot land ahead of what it is publishing.
fn fill(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The name a file takes while it is being written to become `path`: in the
/// same directory, and shared with nothing else being written anywhere.
fn beside(path: &Path) -> io::Result<PathBuf> {
    static WRITES: AtomicU64 = AtomicU64::new(0);

    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a document needs a file to be written to",
        )
    })?;
    let (before, after) = TEMPORARY;
    let mut temporary = OsString::from(before);
    temporary.push(name);
    temporary.push(format!(
        "{after}{}-{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(path.with_file_name(temporary))
}

/// Makes sure a directory named for outputs exists.
pub fn make_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("{} cannot be created", directory.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own, emptied before it is used.
    fn work_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("scribe-io-{name}"));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("a working directory can be made");
        directory
    }

    /// The names in a directory, sorted.
    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("the directory can be read")
            .map(|entry| entry.expect("the directory can be read").file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_takes_the_place_of_the_one_that_was_there() {
        let directory = work_directory("replace");
        let target = directory.join("page.svg");
        Destination::File(target.clone())
            .write(b"first")
            .expect("the first document is written");
        Destination::File(target.clone())
            .write(b"second")
            .expect("the second document is written");
        assert_eq!(fs::read(&target).expect("the document is there"), b"second");
        assert_eq!(names(&directory), ["page.svg"], "nothing is left over");
    }

    #[test]
    fn a_write_that_fails_leaves_the_target_alone_and_nothing_beside_it() {
        let directory = work_directory("failed-write");
        let target = directory.join("page.svg");
        fs::create_dir(&target).expect("the target can be made a directory");
        fs::write(target.join("inside"), b"kept").expect("the directory can be written into");

        let error = Destination::File(target.clone())
            .write(b"anything")
            .expect_err("a directory cannot be replaced by a file");
        assert!(error.to_string().contains("cannot be written"));

        assert_eq!(names(&target), ["inside"], "the target is untouched");
        assert_eq!(names(&directory), ["page.svg"], "nothing is left behind");
    }

    #[test]
    fn two_writes_to_one_path_are_never_written_through_the_same_file() {
        let path = Path::new("out/page.svg");
        assert_ne!(
            beside(path).expect("a name is made"),
            beside(path).expect("a name is made")
        );
    }

    #[test]
    fn a_path_that_names_no_file_is_refused_rather_than_written_beside() {
        assert!(beside(Path::new("..")).is_err());
    }

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
