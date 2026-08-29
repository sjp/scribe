//! What the command line accepts.
//!
//! The renderers decide their own options, so this module deliberately knows
//! very little about them: `--format` names one and `--opt name=value` sets
//! anything it takes. The handful of named flags are shorthands for the
//! options people reach for most, and each one says which option it stands
//! for so that a format without it can say so.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};
use scribe_core::render::OptionValue;

/// The environment variable naming the text detection model.
pub const DETECTION_VARIABLE: &str = "SCRIBE_DETECTION_MODEL";

/// The environment variable naming the text recognition model.
pub const RECOGNITION_VARIABLE: &str = "SCRIBE_RECOGNITION_MODEL";

/// The format rendered when nothing chooses one.
pub const DEFAULT_FORMAT: &str = "svg";

/// The format `--template-file` chooses on its own.
pub const TEMPLATE_FORMAT: &str = "template";

/// The path that stands for standard input or standard output.
pub const STREAM: &str = "-";

/// Make the text in raster images searchable and selectable.
#[derive(Debug, Parser)]
#[command(name = "scribe", version, about, long_about = None)]
pub struct Cli {
    /// What to do.
    #[command(subcommand)]
    pub command: Command,

    /// How much to say while doing it.
    #[command(flatten)]
    pub logging: Logging,
}

/// How much the run reports about itself.
#[derive(Debug, Args)]
pub struct Logging {
    /// Report what each step did and how long it took; repeat for more.
    #[arg(
        short,
        long,
        global = true,
        action = ArgAction::Count,
        conflicts_with = "quiet",
        display_order = 900
    )]
    pub verbose: u8,

    /// Report nothing but errors.
    #[arg(short, long, global = true, display_order = 901)]
    pub quiet: bool,
}

/// The things scribe can be asked to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read the text in one or more images and render it.
    Ocr(OcrCommand),

    /// Render layouts that were read earlier, without running recognition.
    Render(RenderCommand),

    /// List the output formats and the options each one takes.
    Formats,

    /// List the templates that ship with scribe.
    Templates,

    /// Print the JSON Schema of the layout model.
    Schema,
}

/// Recognise the text in images and render the result.
#[derive(Debug, Args)]
#[command(after_help = "\
Options belonging to a format are set with `--opt name=value`, which may be \
given more than once and wins over the flags that stand for particular \
options. Run `scribe formats` to see what each format takes.

Models are never downloaded. Pass them with --detection-model and \
--recognition-model, or set SCRIBE_DETECTION_MODEL and \
SCRIBE_RECOGNITION_MODEL.")]
pub struct OcrCommand {
    /// The images to read; `-` reads one image from standard input.
    #[arg(value_name = "IMAGE", required = true)]
    pub images: Vec<PathBuf>,

    /// The models to run.
    #[command(flatten)]
    pub models: ModelArgs,

    /// How to run them.
    #[command(flatten)]
    pub recognition: RecognitionArgs,

    /// Where the results go.
    #[command(flatten)]
    pub output: OutputArgs,

    /// What the results look like.
    #[command(flatten)]
    pub render: RenderArgs,
}

/// Render layouts read earlier.
#[derive(Debug, Args)]
#[command(after_help = "\
Options belonging to a format are set with `--opt name=value`, which may be \
given more than once and wins over the flags that stand for particular \
options. Run `scribe formats` to see what each format takes.")]
pub struct RenderCommand {
    /// The layouts to render, as `--format json` writes them; `-` reads one
    /// from standard input.
    #[arg(value_name = "LAYOUT", required = true)]
    pub layouts: Vec<PathBuf>,

    /// The image the layouts were read from, for a format that embeds or
    /// links to it; one layout only.
    #[arg(long, value_name = "PATH")]
    pub image: Option<PathBuf>,

    /// Write the output here instead of to standard output; one layout only.
    #[arg(short, long, value_name = "PATH", conflicts_with = "out_dir")]
    pub out: Option<PathBuf>,

    /// Write one output per layout into this directory, each named after its
    /// layout.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// What the results look like.
    #[command(flatten)]
    pub render: RenderArgs,
}

/// The trained models recognition runs.
#[derive(Debug, Args)]
pub struct ModelArgs {
    /// The text detection model, an `.rten` file [env: SCRIBE_DETECTION_MODEL]
    #[arg(long, value_name = "PATH")]
    pub detection_model: Option<PathBuf>,

    /// The text recognition model, an `.rten` file [env: SCRIBE_RECOGNITION_MODEL]
    #[arg(long, value_name = "PATH")]
    pub recognition_model: Option<PathBuf>,
}

/// How the recogniser reads what detection found.
#[derive(Debug, Args)]
pub struct RecognitionArgs {
    /// The characters the recognition model was trained on, in the order it
    /// was trained on them; only for a model of your own.
    #[arg(long, value_name = "CHARS")]
    pub alphabet: Option<String>,

    /// Recognise only these characters, such as the digits alone.
    #[arg(long, value_name = "CHARS")]
    pub allowed_chars: Option<String>,

    /// Keep this many candidate readings alive instead of taking the
    /// likeliest character at every step; slower, sometimes more accurate.
    #[arg(long, value_name = "N")]
    pub beam_width: Option<u32>,

    /// Leave the per-character boxes out of the layout, which is most of its
    /// size.
    #[arg(long)]
    pub no_chars: bool,
}

/// Where the outputs of an `ocr` run are written.
#[derive(Debug, Args)]
pub struct OutputArgs {
    /// Write the output here instead of to standard output; one image only.
    #[arg(short, long, value_name = "PATH", conflicts_with = "out_dir")]
    pub out: Option<PathBuf>,

    /// Write one output per image into this directory, each named after its
    /// image.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Also write the layout the output was rendered from: beside each
    /// output when given no path, or to that path for a single image.
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    pub layout_json: Option<Option<PathBuf>>,
}

/// What an output looks like, whichever command produces it.
#[derive(Debug, Args)]
pub struct RenderArgs {
    /// The output format: `svg`, `json`, `template` or any other the build
    /// knows [default: svg, or template with --template-file]
    #[arg(long, value_name = "NAME")]
    pub format: Option<String>,

    /// Set one option of the chosen format, as `name=value`; repeatable, and
    /// it wins over the flags that stand for particular options.
    #[arg(long = "opt", value_name = "NAME=VALUE")]
    pub opts: Vec<String>,

    /// Render through a Jinja template of your own, choosing the `template`
    /// format unless `--format` says otherwise.
    #[arg(long, value_name = "PATH")]
    pub template_file: Option<PathBuf>,

    /// Draw the text over the image instead of leaving it transparent.
    #[arg(long, conflicts_with = "debug")]
    pub visible: bool,

    /// Draw the text and outline the boxes it was recognised from.
    #[arg(long)]
    pub debug: bool,

    /// Carry the image inside the output.
    #[arg(long, conflicts_with_all = ["link", "no_image"])]
    pub embed: bool,

    /// Point the output at the image instead of carrying it.
    #[arg(long, conflicts_with = "no_image")]
    pub link: bool,

    /// Leave the image out, so the text layer can be laid over one elsewhere.
    #[arg(long)]
    pub no_image: bool,

    /// What `--link` points at; the image's path as given when unset.
    #[arg(long, value_name = "HREF")]
    pub link_href: Option<String>,

    /// Leave out words the recogniser is less sure of than this, from 0 to 1.
    #[arg(long, value_name = "CONFIDENCE")]
    pub min_confidence: Option<f64>,
}

/// One flag standing in for an option of the chosen format.
///
/// The flag is carried alongside the option so that a format without that
/// option can be refused in the words the person typed.
#[derive(Clone, Debug)]
pub struct Shorthand {
    /// The flag as it was written.
    pub flag: &'static str,
    /// The option it sets.
    pub option: &'static str,
    /// What it sets it to.
    pub value: OptionValue,
}

impl RenderArgs {
    /// The format to render, from `--format`, from `--template-file`, or the
    /// default.
    pub fn format(&self) -> &str {
        match (&self.format, &self.template_file) {
            (Some(name), _) => name,
            (None, Some(_)) => TEMPLATE_FORMAT,
            (None, None) => DEFAULT_FORMAT,
        }
    }

    /// The options the named flags stand for, in the order they are applied.
    ///
    /// `template_source` is not among them: it is the contents of a file, so
    /// it is added by whoever reads that file.
    pub fn shorthands(&self) -> Vec<Shorthand> {
        let mut set = Vec::new();
        let mut add = |flag, option, value: OptionValue| {
            set.push(Shorthand {
                flag,
                option,
                value,
            });
        };
        if self.debug {
            add("--debug", "text_mode", "debug".into());
        } else if self.visible {
            add("--visible", "text_mode", "visible".into());
        }
        if self.embed {
            add("--embed", "image_mode", "embed".into());
        } else if self.link {
            add("--link", "image_mode", "link".into());
        } else if self.no_image {
            add("--no-image", "image_mode", "none".into());
        }
        if let Some(confidence) = self.min_confidence {
            add("--min-confidence", "min_confidence", confidence.into());
        }
        set
    }
}
