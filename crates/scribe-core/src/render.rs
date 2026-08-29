//! Turning a layout into an output format.
//!
//! A renderer is pure: it takes a layout, its own options and optional access
//! to the source image, and returns a string or bytes. Nothing is written and
//! nothing is fetched. Built-in renderers cover JSON, SVG and user-supplied
//! templates; the SVG default produces an image that looks exactly like the
//! original with a transparent but selectable text layer over it.
//!
//! Renderers are reached through a [`Registry`] and configured through
//! [`Options`], a string-keyed bag of values. Neither the caller nor the
//! registry knows what any particular renderer takes: a renderer publishes
//! its options with [`Renderer::describe_options`], and a command line or a
//! JavaScript object can be handed straight through. Values arriving as
//! strings are converted to the kind the renderer asked for, so `pretty=false`
//! typed at a terminal means the same as `false` passed from JavaScript.
//!
//! ```
//! use scribe_core::image_source::ImageSource;
//! use scribe_core::layout::Layout;
//! use scribe_core::render::{Options, registry};
//!
//! let registry = registry();
//! let renderer = registry.get("json").expect("json is built in");
//! let output = renderer
//!     .render(
//!         &Layout::empty(8, 4),
//!         &ImageSource::new(8, 4),
//!         &Options::from_iter([("pretty", false)]),
//!     )
//!     .expect("an empty layout renders");
//! assert_eq!(output.mime, "application/json");
//! assert_eq!(
//!     output.as_str(),
//!     Some(r#"{"version":1,"image":{"width":8,"height":4},"lines":[]}"#)
//! );
//! ```

mod json;
mod svg;
mod template;

pub use json::JsonRenderer;
pub use svg::SvgRenderer;
pub use template::{TemplateRenderer, list_templates};

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use thiserror::Error;

use crate::image_source::ImageSource;
use crate::layout::Layout;

/// The cause of a failure inside a renderer's own machinery, whose error
/// types are not part of this crate's interface.
pub type OutputError = Box<dyn std::error::Error + Send + Sync>;

/// A rendered document, ready to be written somewhere this crate knows
/// nothing about.
///
/// The media type and the extension are borrowed or owned as the renderer
/// finds convenient: one whose output is always the same format names it with
/// a literal, while one rendering whatever format a caller's template
/// describes works its out at render time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOutput {
    /// The document itself. Text formats are UTF-8.
    pub bytes: Vec<u8>,
    /// The media type of the document, such as `application/json`.
    pub mime: Cow<'static, str>,
    /// The customary file extension, without a dot, such as `json`.
    pub extension: Cow<'static, str>,
}

impl RenderOutput {
    /// A document of the given bytes, media type and extension.
    pub fn new(
        bytes: Vec<u8>,
        mime: impl Into<Cow<'static, str>>,
        extension: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            bytes,
            mime: mime.into(),
            extension: extension.into(),
        }
    }

    /// A document that is text, stored as its UTF-8 bytes.
    pub fn text(
        text: impl Into<String>,
        mime: impl Into<Cow<'static, str>>,
        extension: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::new(text.into().into_bytes(), mime, extension)
    }

    /// The document as text, or `None` if this renderer produced bytes that
    /// are not UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }
}

/// The kind of value an option takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    /// True or false.
    Bool,
    /// A whole number.
    Int,
    /// A number, whole or not.
    Float,
    /// Text.
    Str,
}

impl fmt::Display for OptionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bool => "true or false",
            Self::Int => "a whole number",
            Self::Float => "a number",
            Self::Str => "text",
        })
    }
}

/// The value of one option.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionValue {
    /// True or false.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A number, whole or not.
    Float(f64),
    /// Text.
    Str(String),
}

impl OptionValue {
    /// Which kind of value this is.
    pub fn kind(&self) -> OptionKind {
        match self {
            Self::Bool(_) => OptionKind::Bool,
            Self::Int(_) => OptionKind::Int,
            Self::Float(_) => OptionKind::Float,
            Self::Str(_) => OptionKind::Str,
        }
    }

    /// The same value as the given kind, or `None` if it cannot be one.
    ///
    /// A caller that cannot know what a renderer expects — a command line
    /// passing `name=value`, or JavaScript, where every number is a float —
    /// gets its value read the way the renderer meant it.
    pub fn coerce(&self, kind: OptionKind) -> Option<Self> {
        Some(match (kind, self) {
            (OptionKind::Bool, Self::Bool(value)) => Self::Bool(*value),
            (OptionKind::Bool, Self::Str(text)) => Self::Bool(parse_bool(text)?),
            (OptionKind::Int, Self::Int(value)) => Self::Int(*value),
            (OptionKind::Int, Self::Float(value)) => Self::Int(whole(*value)?),
            (OptionKind::Int, Self::Str(text)) => Self::Int(text.trim().parse().ok()?),
            (OptionKind::Float, Self::Float(value)) => Self::Float(*value),
            (OptionKind::Float, Self::Int(value)) => Self::Float(*value as f64),
            (OptionKind::Float, Self::Str(text)) => Self::Float(text.trim().parse().ok()?),
            (OptionKind::Str, Self::Str(text)) => Self::Str(text.clone()),
            (OptionKind::Str, Self::Bool(value)) => Self::Str(value.to_string()),
            (OptionKind::Str, Self::Int(value)) => Self::Str(value.to_string()),
            (OptionKind::Str, Self::Float(value)) => Self::Str(value.to_string()),
            _ => return None,
        })
    }
}

impl fmt::Display for OptionValue {
    /// Spells the value the way it would be written in an error message,
    /// with text quoted so an empty or padded string is visible.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Str(text) => write!(f, "{text:?}"),
        }
    }
}

impl From<bool> for OptionValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for OptionValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for OptionValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for OptionValue {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<&str> for OptionValue {
    fn from(value: &str) -> Self {
        Self::Str(value.to_string())
    }
}

/// Reads the ways a person might write a boolean at a command line.
pub(crate) fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// A float as a whole number, or `None` if it has a fractional part or does
/// not fit.
fn whole(value: f64) -> Option<i64> {
    (value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64)
        .then_some(value as i64)
}

/// One option a renderer takes.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionSpec {
    /// The name it is set by.
    pub name: &'static str,
    /// The kind of value it takes.
    pub kind: OptionKind,
    /// What it is when nobody sets it.
    pub default: OptionValue,
    /// A sentence describing it, fit to show in a help listing.
    pub help: &'static str,
    /// The words it accepts, when it accepts only a few of them; empty when
    /// any value of its kind will do.
    pub choices: &'static [&'static str],
}

impl OptionSpec {
    /// Describes an option of the given name, kind, default and help text,
    /// taking any value of that kind.
    pub fn new(
        name: &'static str,
        kind: OptionKind,
        default: OptionValue,
        help: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            default,
            help,
            choices: &[],
        }
    }

    /// The same option, narrowed to the given words.
    ///
    /// A caller offering the option can list them, and one setting it to
    /// anything else is told what it could have said instead.
    #[must_use]
    pub fn with_choices(mut self, choices: &'static [&'static str]) -> Self {
        self.choices = choices;
        self
    }
}

/// Options set by a caller, by name.
///
/// The keys are strings so that a command line can accept `name=value` for
/// any renderer and a JavaScript caller can pass a plain object, neither of
/// them knowing what options exist. Whether a name means anything, and
/// whether its value is of a usable kind, is settled by the renderer when it
/// resolves them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options(BTreeMap<String, OptionValue>);

impl Options {
    /// No options at all, leaving every renderer at its defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets one option, replacing any previous value of that name.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<OptionValue>) {
        self.0.insert(name.into(), value.into());
    }

    /// The same options with one more set, for building them in a chain.
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<OptionValue>) -> Self {
        self.set(name, value);
        self
    }

    /// The value set for a name, if any.
    pub fn get(&self, name: &str) -> Option<&OptionValue> {
        self.0.get(name)
    }

    /// Whether nothing has been set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every name and value that has been set, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &OptionValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// Checks these options against the ones a renderer takes and fills in
    /// the defaults for those left unset.
    ///
    /// This is what [`Renderer::resolve_options`] does; call it directly only
    /// when resolving options against something other than the renderer that
    /// declared them.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first option that the renderer does not
    /// take, or whose value is not of the kind it takes.
    pub fn resolve(
        &self,
        renderer: &str,
        specs: &[OptionSpec],
    ) -> Result<ResolvedOptions, RenderError> {
        let mut values: BTreeMap<&'static str, OptionValue> = specs
            .iter()
            .map(|spec| (spec.name, spec.default.clone()))
            .collect();
        for (name, value) in self.iter() {
            let spec = specs.iter().find(|spec| spec.name == name).ok_or_else(|| {
                RenderError::UnknownOption {
                    renderer: renderer.to_string(),
                    name: name.to_string(),
                    known: specs.iter().map(|spec| spec.name).collect(),
                }
            })?;
            let value = value
                .coerce(spec.kind)
                .ok_or_else(|| RenderError::InvalidOption {
                    renderer: renderer.to_string(),
                    name: spec.name,
                    expected: spec.kind,
                    actual: value.clone(),
                })?;
            if let OptionValue::Str(text) = &value
                && !spec.choices.is_empty()
                && !spec.choices.contains(&text.as_str())
            {
                return Err(RenderError::InvalidChoice {
                    renderer: renderer.to_string(),
                    name: spec.name,
                    choices: spec.choices,
                    actual: text.clone(),
                });
            }
            values.insert(spec.name, value);
        }
        Ok(ResolvedOptions { values })
    }
}

impl<N, V> FromIterator<(N, V)> for Options
where
    N: Into<String>,
    V: Into<OptionValue>,
{
    fn from_iter<I: IntoIterator<Item = (N, V)>>(options: I) -> Self {
        Self(
            options
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        )
    }
}

/// Options that are known to be a renderer's own, every one of them set.
///
/// A renderer resolves the caller's options once and then reads each of its
/// own by name, knowing that it is there and of the right kind.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedOptions {
    values: BTreeMap<&'static str, OptionValue>,
}

impl ResolvedOptions {
    /// The value of one of the renderer's options.
    ///
    /// # Panics
    ///
    /// Panics if the renderer did not declare an option of that name, which
    /// is a mistake in the renderer rather than in its caller.
    pub fn get(&self, name: &str) -> &OptionValue {
        self.values
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` is not one of this renderer's own options"))
    }

    /// The value of a `Bool` option.
    ///
    /// # Panics
    ///
    /// As [`ResolvedOptions::get`], and also if the option is of another
    /// kind.
    pub fn bool(&self, name: &str) -> bool {
        match self.get(name) {
            OptionValue::Bool(value) => *value,
            other => panic!("`{name}` is {other}, not true or false"),
        }
    }

    /// The value of an `Int` option.
    ///
    /// # Panics
    ///
    /// As [`ResolvedOptions::bool`].
    pub fn int(&self, name: &str) -> i64 {
        match self.get(name) {
            OptionValue::Int(value) => *value,
            other => panic!("`{name}` is {other}, not a whole number"),
        }
    }

    /// The value of a `Float` option.
    ///
    /// # Panics
    ///
    /// As [`ResolvedOptions::bool`].
    pub fn float(&self, name: &str) -> f64 {
        match self.get(name) {
            OptionValue::Float(value) => *value,
            other => panic!("`{name}` is {other}, not a number"),
        }
    }

    /// The value of a `Str` option.
    ///
    /// # Panics
    ///
    /// As [`ResolvedOptions::bool`].
    pub fn str(&self, name: &str) -> &str {
        match self.get(name) {
            OptionValue::Str(value) => value,
            other => panic!("`{name}` is {other}, not text"),
        }
    }
}

/// Everything that can go wrong between a layout and an output.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RenderError {
    /// An option was set that the renderer does not take.
    #[error(
        "the {renderer} renderer has no `{name}` option; it takes {}",
        list(known)
    )]
    UnknownOption {
        /// The renderer the options were meant for.
        renderer: String,
        /// The name that was set.
        name: String,
        /// The names it does take.
        known: Vec<&'static str>,
    },

    /// An option was set to a value it cannot take.
    #[error("the {renderer} renderer's `{name}` option takes {expected}, but was given {actual}")]
    InvalidOption {
        /// The renderer the options were meant for.
        renderer: String,
        /// The option that was set.
        name: &'static str,
        /// The kind of value it takes.
        expected: OptionKind,
        /// The value it was given.
        actual: OptionValue,
    },

    /// An option that takes one of a few words was set to another.
    #[error(
        "the {renderer} renderer's `{name}` option takes one of {}, but was given {:?}",
        list(choices),
        actual
    )]
    InvalidChoice {
        /// The renderer the options were meant for.
        renderer: String,
        /// The option that was set.
        name: &'static str,
        /// The words it takes.
        choices: &'static [&'static str],
        /// The word it was given.
        actual: String,
    },

    /// The renderer was asked for an output needing something about the
    /// source image that the caller did not supply.
    #[error("the {renderer} renderer was asked to {intent}, but {missing}")]
    MissingImage {
        /// The renderer that could not go on.
        renderer: String,
        /// What it was asked to do, such as `embed the source image`.
        intent: &'static str,
        /// What was lacking, such as `no path or URL for it is known`.
        missing: &'static str,
    },

    /// A template could not be read, or failed while it was being rendered.
    #[error("the {renderer} renderer could not use {template}{}: {detail}", at(*line, *column))]
    Template {
        /// The renderer that could not go on.
        renderer: String,
        /// Which template it was, such as ``the `hocr` template``.
        template: String,
        /// The line the failure was reported at, counting from one.
        line: Option<usize>,
        /// The column within that line, counting from one.
        column: Option<usize>,
        /// What the templating engine said, without its own idea of where.
        detail: String,
        /// The engine's own error, for a caller that wants more of it.
        #[source]
        source: OutputError,
    },

    /// The renderer had everything it needed but could not produce a
    /// document.
    #[error("the {renderer} renderer could not write its output")]
    Write {
        /// The renderer that failed.
        renderer: String,
        /// What it said.
        #[source]
        source: OutputError,
    },
}

impl RenderError {
    /// The failure of a renderer to write what it was asked for.
    pub fn write(renderer: &str, source: impl Into<OutputError>) -> Self {
        Self::Write {
            renderer: renderer.to_string(),
            source: source.into(),
        }
    }

    /// A renderer's need for part of the source image that it was not given.
    pub fn missing_image(renderer: &str, intent: &'static str, missing: &'static str) -> Self {
        Self::MissingImage {
            renderer: renderer.to_string(),
            intent,
            missing,
        }
    }
}

/// Spells where in a template something went wrong, and nothing at all when
/// that is not known.
fn at(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!(", line {line} column {column}"),
        (Some(line), None) => format!(", line {line}"),
        _ => String::new(),
    }
}

/// Writes a number with at most `precision` decimals, dropping the trailing
/// zeros so that a whole pixel reads as a whole number.
///
/// A number no document format can hold is written as zero rather than as
/// `NaN` or `inf`, which no reader of one would accept.
pub(crate) fn number(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let text = format!("{value:.precision$}");
    let trimmed = match text.split_once('.') {
        Some(_) => text.trim_end_matches('0').trim_end_matches('.'),
        None => text.as_str(),
    };
    match trimmed {
        "" | "-0" => "0".to_string(),
        trimmed => trimmed.to_string(),
    }
}

/// Spells a list of option names for an error message.
fn list(names: &[&str]) -> String {
    if names.is_empty() {
        return "no options".to_string();
    }
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One way of turning a layout into a document.
///
/// Implement this to add an output format of your own and [register
/// it](Registry::register); the command line and the JavaScript bindings
/// will offer it and its options alongside the built-in ones.
pub trait Renderer {
    /// The name this renderer is chosen by, such as `json`.
    fn name(&self) -> &str;

    /// The options it takes, with their kinds, defaults and help text.
    ///
    /// This is the whole of what a caller needs in order to offer the
    /// renderer's options without knowing anything else about it.
    fn describe_options(&self) -> Vec<OptionSpec>;

    /// Turns a layout into a document.
    ///
    /// The image is what the layout was read from; a renderer may embed it,
    /// link to it, or ignore it entirely.
    ///
    /// # Errors
    ///
    /// Returns an error if the options are not ones this renderer takes, or
    /// if the document cannot be written.
    fn render(
        &self,
        layout: &Layout,
        image: &ImageSource<'_>,
        options: &Options,
    ) -> Result<RenderOutput, RenderError>;

    /// Checks the caller's options against this renderer's own and fills in
    /// the defaults, ready to be read by name.
    ///
    /// # Errors
    ///
    /// As [`Options::resolve`].
    fn resolve_options(&self, options: &Options) -> Result<ResolvedOptions, RenderError> {
        options.resolve(self.name(), &self.describe_options())
    }
}

/// The renderers something can choose between, by name.
#[derive(Default)]
pub struct Registry {
    renderers: BTreeMap<String, Box<dyn Renderer>>,
}

impl Registry {
    /// A registry with nothing in it. See [`registry`] for one with the
    /// built-in renderers already in place.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a renderer under its own name, returning whichever renderer that
    /// name stood for before, so a caller can replace a built-in with its
    /// own.
    pub fn register(&mut self, renderer: Box<dyn Renderer>) -> Option<Box<dyn Renderer>> {
        self.renderers.insert(renderer.name().to_string(), renderer)
    }

    /// The renderer of that name, if there is one.
    pub fn get(&self, name: &str) -> Option<&dyn Renderer> {
        self.renderers.get(name).map(AsRef::as_ref)
    }

    /// The names of every renderer, in alphabetical order.
    pub fn names(&self) -> Vec<&str> {
        self.renderers.keys().map(String::as_str).collect()
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Registry").field(&self.names()).finish()
    }
}

/// The built-in renderers.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(Box::new(JsonRenderer));
    registry.register(Box::new(SvgRenderer));
    registry.register(Box::new(TemplateRenderer));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A renderer that takes one of each kind of option and reports what it
    /// resolved them to.
    struct Everything;

    impl Renderer for Everything {
        fn name(&self) -> &str {
            "everything"
        }

        fn describe_options(&self) -> Vec<OptionSpec> {
            vec![
                OptionSpec::new("flag", OptionKind::Bool, OptionValue::Bool(true), "A flag."),
                OptionSpec::new("count", OptionKind::Int, OptionValue::Int(3), "A count."),
                OptionSpec::new(
                    "ratio",
                    OptionKind::Float,
                    OptionValue::Float(0.5),
                    "A ratio.",
                ),
                OptionSpec::new(
                    "label",
                    OptionKind::Str,
                    OptionValue::Str("text".to_string()),
                    "A label.",
                ),
            ]
        }

        fn render(
            &self,
            _layout: &Layout,
            _image: &ImageSource<'_>,
            options: &Options,
        ) -> Result<RenderOutput, RenderError> {
            let options = self.resolve_options(options)?;
            Ok(RenderOutput::text(
                format!(
                    "{} {} {} {}",
                    options.bool("flag"),
                    options.int("count"),
                    options.float("ratio"),
                    options.str("label"),
                ),
                "text/plain",
                "txt",
            ))
        }
    }

    fn render(options: Options) -> Result<String, RenderError> {
        Everything
            .render(&Layout::empty(4, 2), &ImageSource::new(4, 2), &options)
            .map(|output| output.as_str().expect("the output is text").to_string())
    }

    #[test]
    fn unset_options_take_their_defaults() {
        assert_eq!(render(Options::new()).unwrap(), "true 3 0.5 text");
    }

    #[test]
    fn set_options_are_used() {
        let options = Options::new()
            .with("flag", false)
            .with("count", 7_i64)
            .with("ratio", 1.25)
            .with("label", "other");
        assert_eq!(render(options).unwrap(), "false 7 1.25 other");
    }

    #[test]
    fn values_written_as_text_are_read_as_the_kind_the_renderer_takes() {
        let options = Options::new()
            .with("flag", "off")
            .with("count", " 7 ")
            .with("ratio", "1.25")
            .with("label", "other");
        assert_eq!(render(options).unwrap(), "false 7 1.25 other");

        for (text, expected) in [("TRUE", true), ("yes", true), ("1", true), ("no", false)] {
            assert_eq!(
                render(Options::new().with("flag", text)).unwrap(),
                format!("{expected} 3 0.5 text"),
                "`{text}` should read as {expected}"
            );
        }
    }

    #[test]
    fn whole_numbers_pass_for_either_kind_of_number() {
        // JavaScript has only one kind of number, so a count arrives as a
        // float and a ratio may arrive as an integer.
        let options = Options::new().with("count", 7.0).with("ratio", 2_i64);
        assert_eq!(render(options).unwrap(), "true 7 2 text");
    }

    #[test]
    fn an_unknown_option_names_itself_and_the_ones_that_exist() {
        let error = render(Options::new().with("colour", "red")).unwrap_err();
        assert!(matches!(
            &error,
            RenderError::UnknownOption { renderer, name, .. }
                if renderer == "everything" && name == "colour"
        ));
        let message = error.to_string();
        assert!(message.contains("`colour`"), "{message}");
        assert!(
            message.contains("`flag`, `count`, `ratio`, `label`"),
            "{message}"
        );
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_rejected() {
        let error = render(Options::new().with("flag", "maybe")).unwrap_err();
        assert!(matches!(
            &error,
            RenderError::InvalidOption {
                name: "flag",
                expected: OptionKind::Bool,
                ..
            }
        ));
        let message = error.to_string();
        assert!(message.contains("\"maybe\""), "{message}");

        assert!(matches!(
            render(Options::new().with("count", 1.5)).unwrap_err(),
            RenderError::InvalidOption {
                name: "count",
                expected: OptionKind::Int,
                ..
            }
        ));
        assert!(matches!(
            render(Options::new().with("ratio", true)).unwrap_err(),
            RenderError::InvalidOption { name: "ratio", .. }
        ));
    }

    #[test]
    fn an_option_narrowed_to_a_few_words_takes_only_those() {
        let specs = [OptionSpec::new(
            "gait",
            OptionKind::Str,
            OptionValue::Str("walk".to_string()),
            "How to get there.",
        )
        .with_choices(&["walk", "run"])];
        let resolve = |value| {
            Options::new()
                .with("gait", value)
                .resolve("courier", &specs)
        };

        assert_eq!(resolve("run").unwrap().str("gait"), "run");
        assert_eq!(
            Options::new()
                .resolve("courier", &specs)
                .unwrap()
                .str("gait"),
            "walk"
        );

        let error = resolve("fly").unwrap_err();
        assert!(matches!(
            &error,
            RenderError::InvalidChoice { name: "gait", actual, .. } if actual == "fly"
        ));
        assert_eq!(
            error.to_string(),
            "the courier renderer's `gait` option takes one of `walk`, `run`, \
             but was given \"fly\""
        );
    }

    #[test]
    fn a_renderer_with_no_options_says_so() {
        struct Bare;

        impl Renderer for Bare {
            fn name(&self) -> &str {
                "bare"
            }

            fn describe_options(&self) -> Vec<OptionSpec> {
                Vec::new()
            }

            fn render(
                &self,
                _layout: &Layout,
                _image: &ImageSource<'_>,
                options: &Options,
            ) -> Result<RenderOutput, RenderError> {
                self.resolve_options(options)?;
                Ok(RenderOutput::text("", "text/plain", "txt"))
            }
        }

        let error = Bare
            .render(
                &Layout::empty(1, 1),
                &ImageSource::new(1, 1),
                &Options::new().with("anything", 1_i64),
            )
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "the bare renderer has no `anything` option; it takes no options"
        );
    }

    #[test]
    fn options_are_read_back_in_name_order() {
        let options = Options::new().with("zebra", 1_i64).with("aardvark", 2_i64);
        assert!(!options.is_empty());
        assert_eq!(options.get("zebra"), Some(&OptionValue::Int(1)));
        assert_eq!(options.get("missing"), None);
        assert_eq!(
            options.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["aardvark", "zebra"]
        );
        assert!(Options::new().is_empty());
    }

    #[test]
    fn the_built_in_registry_holds_every_built_in_renderer() {
        let registry = registry();
        assert_eq!(registry.names(), ["json", "svg", "template"]);
        assert_eq!(registry.get("json").map(Renderer::name), Some("json"));
        assert_eq!(registry.get("svg").map(Renderer::name), Some("svg"));
        assert_eq!(
            registry.get("template").map(Renderer::name),
            Some("template")
        );
        assert!(registry.get("nothing").is_none());
    }

    #[test]
    fn a_registry_takes_renderers_of_its_own() {
        let mut registry = registry();
        assert!(registry.register(Box::new(Everything)).is_none());
        assert_eq!(registry.names(), ["everything", "json", "svg", "template"]);

        // Registering the same name again replaces what was there, so a
        // caller can put its own renderer in place of a built-in.
        let replaced = registry.register(Box::new(Everything));
        assert_eq!(
            replaced.map(|renderer| renderer.name().to_string()),
            Some("everything".to_string())
        );
        assert_eq!(registry.names(), ["everything", "json", "svg", "template"]);
        assert_eq!(
            format!("{registry:?}"),
            r#"Registry(["everything", "json", "svg", "template"])"#
        );
    }

    #[test]
    fn output_carries_its_type_and_extension() {
        let output = RenderOutput::text("hello", "text/plain", "txt");
        assert_eq!(output.as_str(), Some("hello"));
        assert_eq!((&*output.mime, &*output.extension), ("text/plain", "txt"));
        assert_eq!(
            RenderOutput::new(vec![0xff], "application/octet-stream", "bin").as_str(),
            None
        );
    }
}
