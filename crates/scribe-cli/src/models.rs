//! Finding the trained models on disk and reading them into memory.
//!
//! scribe never fetches a model. A run says where its models are, or the
//! environment does, and anything else is a mistake in the request — one
//! whose message has to be enough for somebody who has not got them yet.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use scribe_core::ocr::{ModelKind, Models};

use crate::cli::{DETECTION_VARIABLE, ModelArgs, RECOGNITION_VARIABLE};
use crate::error::usage;

/// The project the models this engine runs come from.
const PROJECT: &str = "https://github.com/robertknight/ocrs";

/// Where those models are published.
const DOWNLOADS: &str = "https://ocrs-models.s3-accelerate.amazonaws.com";

/// Everything needed to ask for one model and to explain its absence.
struct Wanted {
    /// Which of the two it is.
    kind: ModelKind,
    /// The flag that names it.
    flag: &'static str,
    /// The environment variable that names it instead.
    variable: &'static str,
    /// What it is called where it is published.
    file: &'static str,
}

/// The detection model, and then the recognition model.
const WANTED: [Wanted; 2] = [
    Wanted {
        kind: ModelKind::Detection,
        flag: "--detection-model",
        variable: DETECTION_VARIABLE,
        file: "text-detection.rten",
    },
    Wanted {
        kind: ModelKind::Recognition,
        flag: "--recognition-model",
        variable: RECOGNITION_VARIABLE,
        file: "text-recognition.rten",
    },
];

/// Reads both models, saying where to get them if either is missing.
///
/// # Errors
///
/// Returns a usage error if a model was neither named nor in the
/// environment, and a plain one if a named file cannot be read.
pub fn load(args: &ModelArgs) -> Result<Models> {
    let [detection, recognition] = &WANTED;
    Ok(Models::new(
        read(detection, args.detection_model.as_deref())?,
        read(recognition, args.recognition_model.as_deref())?,
    ))
}

/// Reads one model from wherever the run said it is.
fn read(wanted: &Wanted, given: Option<&Path>) -> Result<Vec<u8>> {
    let path = locate(wanted, given).ok_or_else(|| missing(wanted))?;
    std::fs::read(&path).with_context(|| {
        format!(
            "the {} model at {} cannot be read",
            wanted.kind,
            path.display()
        )
    })
}

/// The path a model was named by, from the flag or from the environment.
///
/// A variable set to nothing counts as unset, so that clearing one in a shell
/// has the effect it looks like it has.
fn locate(wanted: &Wanted, given: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = given {
        return Some(path.to_path_buf());
    }
    let from_environment = std::env::var_os(wanted.variable)?;
    (!from_environment.is_empty()).then(|| PathBuf::from(from_environment))
}

/// The message for a model that was never named.
fn missing(wanted: &Wanted) -> anyhow::Error {
    let Wanted {
        kind,
        flag,
        variable,
        file,
    } = wanted;
    usage(format!(
        "no {kind} model: pass `{flag} <PATH>`, or set {variable}.\n\
         scribe never downloads models. The ones this engine was trained with \
         are published by the ocrs project at {PROJECT} and can be fetched \
         from {DOWNLOADS}/{file}."
    ))
}
