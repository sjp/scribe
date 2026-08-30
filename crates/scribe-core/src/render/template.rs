//! The layout as whatever text format a template describes.
//!
//! Not every useful output is worth a renderer of its own, and no crate can
//! anticipate the format a particular archive, index or page needs. A
//! template is the way out: a Jinja document with the layout, the source
//! image and the caller's own values in scope, rendered to text. The
//! templates that ship with scribe — text layers to lay over an image,
//! transcripts for a screen reader, metadata for a crawler, hOCR, ALTO,
//! Markdown and plain text — are written against that same context, so each
//! one is also a worked example of writing another.
//!
//! A template building a page around an image can ask for the SVG renderer's
//! own text layer rather than writing a second one: `svg` is a function here
//! as well as a renderer, taking the same options by the same names. `scope`
//! is the token that renderer works out for the layout in hand, so that a
//! template can name the wrapper it puts around a layer, and the transcript
//! it puts beside one, what the layer itself is named.
//!
//! Templates are read strictly: naming something that is not there is an
//! error rather than an empty string, and the error says where in the
//! template it happened. Values are escaped as they are written when the
//! output is HTML or XML, which is worked out from its media type and can be
//! set outright.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use minijinja::value::{Enumerator, Kwargs, Object, Value, ValueKind, ViaDeserialize};
use minijinja::{AutoEscape, Environment, Error, ErrorKind, Output, State, UndefinedBehavior};

use super::{
    Layout, OptionKind, OptionSpec, OptionValue, Options, RenderError, RenderOutput, Renderer,
    SvgRenderer, number, parse_bool, scope_token,
};
use crate::image_source::ImageSource;
use crate::layout::RotatedBox;

/// The name this renderer is registered under.
const NAME: &str = "template";

/// What an option name must begin with to be handed to the template as one of
/// `vars` rather than read as an option of this renderer's own.
const VAR_PREFIX: &str = "var.";

/// What a template of the caller's own is called, in the engine's errors and
/// in this crate's.
const GIVEN: &str = "template_source";

/// The media type an output takes when nothing says otherwise.
const DEFAULT_MIME: &str = "text/plain";

/// The file extension an output takes when nothing says otherwise.
const DEFAULT_EXTENSION: &str = "txt";

/// How many decimals a coordinate is written to when a template does not say.
const DEFAULT_PRECISION: usize = 2;

/// The most decimals `round` can be asked for, past which an `f64` has
/// nothing left to say.
const MAX_PRECISION: usize = 17;

/// The apostrophe as HTML writes it, which is also valid XML.
const HTML_APOSTROPHE: &str = "&#39;";

/// The apostrophe as XML writes it, which HTML before version 5 did not know.
const XML_APOSTROPHE: &str = "&apos;";

/// One of the templates that ship with scribe.
#[derive(Debug)]
struct BuiltIn {
    /// The name the `template` option chooses it by.
    name: &'static str,
    /// The template itself, carried in the binary.
    source: &'static str,
    /// The media type of what it writes.
    mime: &'static str,
    /// The customary file extension of what it writes, without a dot.
    extension: &'static str,
}

/// The templates that ship with scribe, in the order they are offered in.
const BUILT_INS: &[BuiltIn] = &[
    BuiltIn {
        name: "html-overlay",
        source: include_str!("../../../../templates/html-overlay.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "svg-overlay",
        source: include_str!("../../../../templates/svg-overlay.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "html-figure",
        source: include_str!("../../../../templates/html-figure.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "sr-only-transcript",
        source: include_str!("../../../../templates/sr-only-transcript.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "figure-transcript",
        source: include_str!("../../../../templates/figure-transcript.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "json-ld",
        source: include_str!("../../../../templates/json-ld.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "layout-json",
        source: include_str!("../../../../templates/layout-json.jinja"),
        mime: "text/html",
        extension: "html",
    },
    BuiltIn {
        name: "hocr",
        source: include_str!("../../../../templates/hocr.jinja"),
        mime: "text/html",
        extension: "hocr",
    },
    BuiltIn {
        name: "alto",
        source: include_str!("../../../../templates/alto.jinja"),
        mime: "application/xml",
        extension: "xml",
    },
    BuiltIn {
        name: "markdown",
        source: include_str!("../../../../templates/markdown.jinja"),
        mime: "text/markdown",
        extension: "md",
    },
    BuiltIn {
        name: "text",
        source: include_str!("../../../../templates/text.jinja"),
        mime: DEFAULT_MIME,
        extension: DEFAULT_EXTENSION,
    },
    BuiltIn {
        name: "alt-text",
        source: include_str!("../../../../templates/alt-text.jinja"),
        mime: DEFAULT_MIME,
        extension: DEFAULT_EXTENSION,
    },
];

/// The same names as [`BUILT_INS`], as the `template` option offers them.
///
/// An option's choices outlive the call that describes them, so they cannot
/// be gathered from [`BUILT_INS`] at the time they are asked for.
const BUILT_IN_NAMES: &[&str] = &[
    "html-overlay",
    "svg-overlay",
    "html-figure",
    "sr-only-transcript",
    "figure-transcript",
    "json-ld",
    "layout-json",
    "hocr",
    "alto",
    "markdown",
    "text",
    "alt-text",
];

/// The ways a template may escape the values it writes, the first being the
/// default.
const ESCAPING: &[&str] = &["auto", "html", "none"];

/// The names of the templates that ship with scribe, in the order the
/// `template` option offers them.
///
/// Each one can be rendered by name, and each one is also an example of the
/// context a template of your own is given.
pub fn list_templates() -> Vec<&'static str> {
    BUILT_INS.iter().map(|built_in| built_in.name).collect()
}

/// Writes the layout through a template, built in or the caller's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TemplateRenderer;

impl Renderer for TemplateRenderer {
    fn name(&self) -> &str {
        NAME
    }

    fn describe_options(&self) -> Vec<OptionSpec> {
        vec![
            OptionSpec::new(
                "template",
                OptionKind::Str,
                OptionValue::Str(BUILT_IN_NAMES[0].to_string()),
                "Which of the built-in templates to render, of the ones listed beside this; ignored when `template_source` is set.",
            )
            .with_choices(BUILT_IN_NAMES),
            OptionSpec::new(
                "template_source",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "A template of your own, in Jinja syntax, rendered instead of a built-in one.",
            ),
            OptionSpec::new(
                "mime",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "The media type of the output; the chosen template's own when empty.",
            ),
            OptionSpec::new(
                "extension",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "The file extension of the output, without a dot; the chosen template's own when empty.",
            ),
            OptionSpec::new(
                "autoescape",
                OptionKind::Str,
                OptionValue::Str(ESCAPING[0].to_string()),
                "Whether values are escaped as they are written; `auto` escapes when the media type is HTML or XML.",
            )
            .with_choices(ESCAPING),
        ]
    }

    /// Renders the layout through the chosen template.
    ///
    /// Options named `var.<name>` are not this renderer's own: they reach the
    /// template as `vars.<name>`, so that a template can take parameters
    /// without this crate knowing what they mean.
    fn render(
        &self,
        layout: &Layout,
        image: &ImageSource<'_>,
        options: &Options,
    ) -> Result<RenderOutput, RenderError> {
        let (own, vars) = split_vars(options);
        let own = self.resolve_options(&own)?;

        let given = own.str("template_source");
        let chosen = if given.is_empty() {
            Some(built_in(own.str("template"))?)
        } else {
            None
        };
        let (name, source) = match chosen {
            Some(built_in) => (built_in.name, built_in.source),
            None => (GIVEN, given),
        };
        let mime = chosen_or(
            own.str("mime"),
            chosen.map_or(DEFAULT_MIME, |built_in| built_in.mime),
        );
        let extension = chosen_or(
            own.str("extension"),
            chosen.map_or(DEFAULT_EXTENSION, |built_in| built_in.extension),
        );

        let image = Arc::new(Image::of(image));
        let mut environment = Environment::new();
        prepare(
            &mut environment,
            escaping(own.str("autoescape"), &mime),
            &image,
            layout,
        );
        environment
            .add_template(name, source)
            .map_err(|error| failure(name, source, error))?;
        let template = environment
            .get_template(name)
            .map_err(|error| failure(name, source, error))?;

        let text = template
            .render(minijinja::context! {
                layout => Value::from_serialize(layout),
                image => Value::from_dyn_object(Arc::clone(&image)),
                text => layout.text(),
                vars => Value::from_iter(vars),
            })
            .map_err(|error| failure(name, source, error))?;
        Ok(RenderOutput::text(text, mime, extension))
    }
}

/// Separates the options this renderer reads from the values it passes
/// through to the template.
///
/// A `var.` with nothing after it is left where it is, so that it is reported
/// as the option that does not exist rather than becoming a nameless value.
fn split_vars(options: &Options) -> (Options, BTreeMap<String, Value>) {
    let mut own = Options::new();
    let mut vars = BTreeMap::new();
    for (name, value) in options.iter() {
        match name
            .strip_prefix(VAR_PREFIX)
            .filter(|name| !name.is_empty())
        {
            Some(name) => {
                vars.insert(name.to_string(), value_of(value));
            }
            None => own.set(name, value.clone()),
        }
    }
    (own, vars)
}

/// One option's value as a template sees it.
fn value_of(value: &OptionValue) -> Value {
    match value {
        OptionValue::Bool(value) => Value::from(*value),
        OptionValue::Int(value) => Value::from(*value),
        OptionValue::Float(value) => Value::from(*value),
        OptionValue::Str(value) => Value::from(value.clone()),
    }
}

/// The built-in template of that name.
///
/// # Errors
///
/// Returns an error listing every name there is when none of them is the one
/// asked for.
fn built_in(name: &str) -> Result<&'static BuiltIn, RenderError> {
    BUILT_INS
        .iter()
        .find(|built_in| built_in.name == name)
        .ok_or_else(|| RenderError::InvalidChoice {
            renderer: NAME.to_string(),
            name: "template",
            choices: BUILT_IN_NAMES,
            actual: name.to_string(),
        })
}

/// What the caller asked for, or what the template says when the caller left
/// it to the template.
fn chosen_or(chosen: &str, fallback: &'static str) -> Cow<'static, str> {
    if chosen.is_empty() {
        Cow::Borrowed(fallback)
    } else {
        Cow::Owned(chosen.to_string())
    }
}

/// Whether values are escaped as a template writes them.
///
/// Left to itself, this follows the media type of the output: a document
/// whose text is marked up needs escaping and one whose text is not would
/// only be disfigured by it.
fn escaping(choice: &str, mime: &str) -> AutoEscape {
    match choice {
        "html" => AutoEscape::Html,
        "none" => AutoEscape::None,
        _ if mime.contains("html") || mime.contains("xml") => AutoEscape::Html,
        _ => AutoEscape::None,
    }
}

/// Sets up the templating engine: how it reads templates, how it writes
/// values, and everything a template can call.
fn prepare(
    environment: &mut Environment<'_>,
    escape: AutoEscape,
    image: &Arc<Image>,
    layout: &Layout,
) {
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.set_debug(true);
    environment.set_keep_trailing_newline(true);
    environment.set_auto_escape_callback(move |_| escape);
    environment.set_formatter(write_value);

    environment.add_filter("xml_escape", xml_escape);
    environment.add_function("xml_escape", xml_escape);
    environment.add_filter("html_escape", html_escape);
    environment.add_function("html_escape", html_escape);
    environment.add_filter("json", json);
    environment.add_function("json", json);
    environment.add_filter("round", round);
    environment.add_function("round", round);
    environment.add_filter("flag", flag);
    environment.add_function("flag", flag);
    environment.add_filter("number", as_number);
    environment.add_function("number", as_number);
    environment.add_filter("rotate_transform", rotate_transform);
    environment.add_function("rotate_transform", rotate_transform);
    environment.add_filter("points", points);
    environment.add_function("points", points);
    environment.add_filter("base64", base64);
    environment.add_function("base64", base64);

    let data_uri = Arc::clone(image);
    environment.add_function("data_uri", move || optional(data_uri.data_uri()));

    // The layer is written from a layout of this function's own: it can be
    // asked for at any point in a template, by which time the borrowed one is
    // gone. The context beside it already holds a copy of the layout, so this
    // is the second rather than the first.
    let layout = Arc::new(layout.clone());

    let scoped = Arc::clone(&layout);
    environment.add_function("scope", move || scope_token(&scoped));

    let image = Arc::clone(image);
    let layer = move |options: Option<Value>, overrides: Kwargs| {
        svg_layer(&layout, &image, options.as_ref(), &overrides)
    };
    environment.add_filter("svg", layer.clone());
    environment.add_function("svg", layer);
}

/// The layout as an SVG document, for a template laying a text layer over an
/// image it is placing itself.
///
/// The options are the SVG renderer's own, given as a mapping, as keywords,
/// or as both with the keywords winning, so that a template can pass the
/// caller's values straight through and still settle what it must. The XML
/// declaration is left out unless it is asked for, since the document is
/// going inside another one.
///
/// The result is marked as needing no escaping: it is markup, and escaping it
/// would put the text of the document into the page instead of the layer.
///
/// # Errors
///
/// Returns an error if a value is not one an option can take, or if the SVG
/// renderer refuses the options, which it does by name.
fn svg_layer(
    layout: &Layout,
    image: &Image,
    options: Option<&Value>,
    overrides: &Kwargs,
) -> Result<Value, Error> {
    let mut settings = Options::new().with("xml_declaration", false);
    if let Some(options) = options {
        read_options(options, &mut settings)?;
    }
    for name in overrides.args() {
        let value: Value = overrides.get(name)?;
        settings.set(name, option_value(&value)?);
    }
    let output = SvgRenderer
        .render(layout, &image.source(), &settings)
        .map_err(|error| {
            Error::new(ErrorKind::InvalidOperation, error.to_string()).with_source(error)
        })?;
    let document = output.as_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            "the svg renderer wrote something that is not text",
        )
    })?;
    Ok(Value::from_safe_string(document.to_string()))
}

/// Reads a mapping of names to values as options for a renderer.
///
/// # Errors
///
/// Returns an error if the value is not a mapping, or if one of its values is
/// not one an option can take.
fn read_options(mapping: &Value, into: &mut Options) -> Result<(), Error> {
    if mapping.is_none() || mapping.is_undefined() {
        return Ok(());
    }
    if mapping.kind() != ValueKind::Map {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("{mapping} is not a mapping of options"),
        ));
    }
    for name in mapping.try_iter()? {
        let value = mapping.get_item(&name)?;
        let name = name.as_str().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("{name} is not the name of an option"),
            )
        })?;
        into.set(name, option_value(&value)?);
    }
    Ok(())
}

/// One value a template gave for a renderer's option, as that renderer reads
/// them.
///
/// # Errors
///
/// Returns an error if the value is not text, a number or a boolean, which
/// are all an option can be.
fn option_value(value: &Value) -> Result<OptionValue, Error> {
    if let Some(text) = value.as_str() {
        return Ok(OptionValue::Str(text.to_string()));
    }
    match value.kind() {
        ValueKind::Bool => Ok(OptionValue::Bool(value.is_true())),
        ValueKind::Number => match value.as_i64() {
            Some(whole) => Ok(OptionValue::Int(whole)),
            None => f64::try_from(value.clone()).map(OptionValue::Float),
        },
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("{value} is not a value an option can take"),
        )),
    }
}

/// Writes one value into the output, escaped when the document is marked up
/// and the value is not already known to be safe in it.
///
/// The engine's own formatter would do this, but escapes `/` as well, which
/// would turn every path and every base64 `data:` URI in an output into a
/// thicket of `&#x2f;` to no end. Standing in for it also means standing in
/// for its refusal to write a value that is not there, which is the whole of
/// what strictness means at this point in a template.
fn write_value(out: &mut Output, state: &State, value: &Value) -> Result<(), Error> {
    if value.is_undefined()
        && matches!(
            state.undefined_behavior(),
            UndefinedBehavior::Strict | UndefinedBehavior::SemiStrict
        )
    {
        return Err(Error::from(ErrorKind::UndefinedError));
    }
    let written = text_of(value);
    let written = match state.auto_escape() {
        AutoEscape::Html if !value.is_safe() => escape(&written, HTML_APOSTROPHE),
        _ => Cow::Borrowed(written.as_ref()),
    };
    out.write_str(&written).map_err(Error::from)
}

/// Replaces the five characters that markup gives a meaning of its own,
/// spelling the apostrophe the way the format being written spells it.
fn escape<'a>(text: &'a str, apostrophe: &str) -> Cow<'a, str> {
    if !text.contains(['&', '<', '>', '"', '\'']) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + text.len() / 8);
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str(apostrophe),
            character => escaped.push(character),
        }
    }
    Cow::Owned(escaped)
}

/// A value's text, which is what it prints as when it is not text already.
fn text_of(value: &Value) -> Cow<'_, str> {
    match value.as_str() {
        Some(text) => Cow::Borrowed(text),
        None => Cow::Owned(value.to_string()),
    }
}

/// Something a template may or may not have been given.
fn optional(value: Option<&str>) -> Value {
    value.map_or_else(|| Value::from_serialize(None::<&str>), Value::from)
}

/// The value escaped for XML, and marked as needing no further escaping.
fn xml_escape(value: Value) -> Value {
    let text = text_of(&value);
    Value::from_safe_string(escape(&text, XML_APOSTROPHE).into_owned())
}

/// The value escaped for HTML, and marked as needing no further escaping.
fn html_escape(value: Value) -> Value {
    let text = text_of(&value);
    Value::from_safe_string(escape(&text, HTML_APOSTROPHE).into_owned())
}

/// The value as JSON, ready to be handed to a script.
///
/// The characters that would end an element early are written as escapes, so
/// that the result is safe to embed in a `<script>` as well as to parse.
///
/// # Errors
///
/// Returns an error if the value holds a number JSON cannot write.
fn json(value: Value) -> Result<Value, Error> {
    let json = serde_json::to_string(&value).map_err(|error| {
        Error::new(
            ErrorKind::BadSerialization,
            "the value cannot be written as JSON",
        )
        .with_source(error)
    })?;
    Ok(Value::from_safe_string(
        json.replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('&', "\\u0026"),
    ))
}

/// A number written with at most `precision` decimals and no trailing zeros,
/// so that a whole pixel reads as `28` rather than `28.0`.
///
/// This stands in for the engine's own `round`, which writes a number back as
/// a number and so cannot drop the zeros a document has no use for.
fn round(value: f64, precision: Option<usize>) -> String {
    number(value, precision.unwrap_or(0).min(MAX_PRECISION))
}

/// A value read as true or false the way a renderer reads its own options,
/// so that a template makes the same sense of `off` typed at a command line
/// as of `false` passed from a program.
///
/// Text that says neither counts as true when there is any of it, which is
/// what a bare `name=` on a command line means.
fn flag(value: Value) -> bool {
    match value.as_str() {
        Some(text) => parse_bool(text).unwrap_or(!text.trim().is_empty()),
        None => value.is_true(),
    }
}

/// A value read as a number, so that a template can reckon with one that
/// reached it as text.
///
/// # Errors
///
/// Returns an error if the value neither is a number nor spells one.
fn as_number(value: Value) -> Result<f64, Error> {
    let read = match value.as_str() {
        Some(text) => text.trim().parse().ok(),
        None => f64::try_from(value.clone()).ok(),
    };
    read.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("{value} is not a number"),
        )
    })
}

/// An oriented box's rotation as an SVG or CSS `rotate(angle cx cy)`.
fn rotate_transform(box_: ViaDeserialize<RotatedBox>, precision: Option<usize>) -> String {
    let precision = precision.unwrap_or(DEFAULT_PRECISION).min(MAX_PRECISION);
    format!(
        "rotate({} {} {})",
        number(box_.angle_deg as f64, precision),
        number(box_.cx as f64, precision),
        number(box_.cy as f64, precision),
    )
}

/// An oriented box's four corners as the `x,y` pairs an SVG `<polygon>`
/// takes, starting at the corner that is the top left before rotation.
fn points(box_: ViaDeserialize<RotatedBox>, precision: Option<usize>) -> String {
    let precision = precision.unwrap_or(DEFAULT_PRECISION).min(MAX_PRECISION);
    box_.corners()
        .iter()
        .map(|(x, y)| {
            format!(
                "{},{}",
                number(*x as f64, precision),
                number(*y as f64, precision)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The value's text as standard base64 with padding.
fn base64(value: Value) -> String {
    BASE64.encode(text_of(&value).as_bytes())
}

/// A failure of the engine as this crate reports it, placed in the template
/// it came from.
fn failure(name: &str, source: &str, error: Error) -> RenderError {
    let (line, column) = position(source, &error);
    let detail = match error.detail() {
        Some(detail) => format!("{}: {detail}", error.kind()),
        None => error.kind().to_string(),
    };
    RenderError::Template {
        renderer: NAME.to_string(),
        template: describe(name),
        line,
        column,
        detail,
        source: Box::new(error),
    }
}

/// How an error names the template that failed.
fn describe(name: &str) -> String {
    match name {
        GIVEN => "the template it was given".to_string(),
        name => format!("the `{name}` template"),
    }
}

/// Where in the template a failure happened, counting lines and columns from
/// one.
///
/// The engine reports the offset of the expression that failed; the line and
/// the column are counted out of the source, since neither is much use
/// without the other in a template whose lines are long.
fn position(source: &str, error: &Error) -> (Option<usize>, Option<usize>) {
    let Some(before) = error.range().and_then(|range| source.get(..range.start)) else {
        return (error.line(), None);
    };
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    (Some(line), Some(column))
}

/// The source image as a template sees it.
///
/// The bytes are copied because a template's values outlive the borrowed
/// [`ImageSource`] they came from; encoding them is not, since that is the
/// expensive part and most templates never ask for it.
#[derive(Debug)]
struct Image {
    width: u32,
    height: u32,
    mime: Option<String>,
    href: Option<String>,
    bytes: Option<Vec<u8>>,
    data_uri: OnceLock<Option<String>>,
}

/// What a template can ask an image for.
const IMAGE_FIELDS: &[&str] = &["width", "height", "mime", "href", "data_uri"];

impl Image {
    /// The same image, held for as long as the template needs it.
    fn of(source: &ImageSource<'_>) -> Self {
        Self {
            width: source.width,
            height: source.height,
            mime: source.mime.map(str::to_string),
            href: source.href.map(str::to_string),
            bytes: source.bytes.map(<[u8]>::to_vec),
            data_uri: OnceLock::new(),
        }
    }

    /// The image as a `data:` URI, or `None` when its bytes or its media type
    /// are not known.
    ///
    /// It is encoded the first time a template asks for it and remembered
    /// after that, so a template that never mentions it never pays for it:
    /// base64 of a photograph is several megabytes of string that a
    /// plain-text output, or one linking to the image by its path, has no use
    /// for.
    fn data_uri(&self) -> Option<&str> {
        self.data_uri
            .get_or_init(|| self.source().data_uri())
            .as_deref()
    }

    /// The same image as a renderer takes it, borrowed from what this one
    /// holds.
    fn source(&self) -> ImageSource<'_> {
        let mut source = ImageSource::new(self.width, self.height);
        source.mime = self.mime.as_deref();
        source.href = self.href.as_deref();
        source.bytes = self.bytes.as_deref();
        source
    }

    /// The value of one of [`IMAGE_FIELDS`].
    fn field(&self, name: &str) -> Option<Value> {
        Some(match name {
            "width" => Value::from(self.width),
            "height" => Value::from(self.height),
            "mime" => optional(self.mime.as_deref()),
            "href" => optional(self.href.as_deref()),
            "data_uri" => optional(self.data_uri()),
            _ => return None,
        })
    }
}

impl Object for Image {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        self.field(key.as_str()?)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(IMAGE_FIELDS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Line, Rect, Word};

    /// The smallest PNG that decoders accept: one opaque black pixel.
    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a,
        0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn sample() -> Layout {
        let bbox = Rect::new(10.0, 20.0, 40.0, 8.0);
        Layout::new(
            80,
            40,
            vec![Line {
                text: "hi there".to_string(),
                bbox,
                rotated_box: RotatedBox::from_rect(bbox),
                words: vec![Word {
                    text: "hi".to_string(),
                    bbox: Rect::new(10.0, 20.0, 16.0, 8.0),
                    rotated_box: RotatedBox::new(18.0, 24.0, 16.0, 8.0, 30.0),
                    chars: Vec::new(),
                    confidence: Some(0.5),
                }],
                confidence: Some(0.5),
            }],
        )
    }

    fn render(options: Options) -> Result<RenderOutput, RenderError> {
        let image = ImageSource::new(80, 40)
            .with_mime("image/png")
            .with_bytes(PIXEL_PNG)
            .with_href("page.png");
        TemplateRenderer.render(&sample(), &image, &options)
    }

    fn rendered(source: &str) -> String {
        let output =
            render(Options::new().with("template_source", source)).expect("the template renders");
        output.as_str().expect("a template writes text").to_string()
    }

    #[test]
    fn a_template_of_ones_own_is_rendered() {
        assert_eq!(rendered("{{ text }}"), "hi there");
    }

    #[test]
    fn a_template_of_ones_own_writes_plain_text_unless_told_otherwise() {
        let output = render(Options::new().with("template_source", "{{ text }}")).unwrap();
        assert_eq!((&*output.mime, &*output.extension), ("text/plain", "txt"));

        let output = render(
            Options::new()
                .with("template_source", "{{ text }}")
                .with("mime", "text/csv")
                .with("extension", "csv"),
        )
        .unwrap();
        assert_eq!((&*output.mime, &*output.extension), ("text/csv", "csv"));
    }

    #[test]
    fn a_built_in_names_the_type_of_what_it_writes() {
        for (template, mime, extension) in [
            ("html-overlay", "text/html", "html"),
            ("svg-overlay", "text/html", "html"),
            ("html-figure", "text/html", "html"),
            ("sr-only-transcript", "text/html", "html"),
            ("figure-transcript", "text/html", "html"),
            ("json-ld", "text/html", "html"),
            ("layout-json", "text/html", "html"),
            ("hocr", "text/html", "hocr"),
            ("alto", "application/xml", "xml"),
            ("markdown", "text/markdown", "md"),
            ("text", "text/plain", "txt"),
            ("alt-text", "text/plain", "txt"),
        ] {
            let output = render(Options::new().with("template", template)).unwrap();
            assert_eq!(
                (&*output.mime, &*output.extension),
                (mime, extension),
                "{template} should say what it writes"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_a_template_lists_the_ones_that_are() {
        let error = render(Options::new().with("template", "yaml")).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("\"yaml\""), "{message}");
        for name in list_templates() {
            assert!(message.contains(&format!("`{name}`")), "{message}");
        }

        // Nothing outside the renderer can reach `built_in` with a name the
        // options let through, but it answers the same way if anything does.
        let message = built_in("yaml").unwrap_err().to_string();
        assert!(message.contains("`html-overlay`"), "{message}");
    }

    #[test]
    fn the_template_option_offers_every_template() {
        let specs = TemplateRenderer.describe_options();
        let spec = specs
            .iter()
            .find(|spec| spec.name == "template")
            .expect("the renderer takes a `template` option");
        // The names are what a caller is shown beside the option, so the
        // help does not spell them a second time.
        assert_eq!(spec.choices, BUILT_IN_NAMES);
        assert_eq!(
            spec.default,
            OptionValue::Str(BUILT_IN_NAMES[0].to_string())
        );
    }

    #[test]
    fn the_names_offered_are_the_templates_there_are() {
        assert_eq!(list_templates(), BUILT_IN_NAMES);
    }

    #[test]
    fn a_broken_template_says_where_it_broke() {
        let error = render(Options::new().with("template_source", "one\ntwo {{ %}")).unwrap_err();
        assert!(
            matches!(
                &error,
                RenderError::Template {
                    line: Some(2),
                    column: Some(8),
                    ..
                }
            ),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("the template it was given"), "{message}");
        assert!(message.contains("line 2 column 8"), "{message}");
        assert!(message.contains("syntax error"), "{message}");
    }

    #[test]
    fn naming_something_that_is_not_there_is_an_error() {
        let error = render(Options::new().with("template_source", "{{ nonsense }}")).unwrap_err();
        assert!(matches!(&error, RenderError::Template { .. }), "{error:?}");
        assert!(error.to_string().contains("undefined"), "{error}");
    }

    #[test]
    fn a_failure_inside_a_built_in_names_it() {
        let error = failure("hocr", "{{ x }}", Error::from(ErrorKind::UndefinedError));
        assert!(error.to_string().contains("the `hocr` template"), "{error}");
    }

    #[test]
    fn values_of_ones_own_reach_the_template() {
        let rendered = render(
            Options::new()
                .with(
                    "template_source",
                    "{{ vars.who }} {{ vars.count }} {{ vars.loud }}",
                )
                .with("var.who", "world")
                .with("var.count", 3_i64)
                .with("var.loud", true),
        )
        .unwrap();
        // The engine spells a boolean the way Jinja always has.
        assert_eq!(rendered.as_str(), Some("world 3 True"));
    }

    #[test]
    fn a_var_with_no_name_is_an_option_that_does_not_exist() {
        let error = render(Options::new().with("var.", "nameless")).unwrap_err();
        assert!(
            matches!(&error, RenderError::UnknownOption { name, .. } if name == "var."),
            "{error:?}"
        );
    }

    #[test]
    fn an_option_that_does_not_exist_is_still_refused() {
        let error = render(Options::new().with("colour", "red")).unwrap_err();
        assert!(
            matches!(&error, RenderError::UnknownOption { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn the_image_is_in_scope() {
        assert_eq!(
            rendered("{{ image.width }}x{{ image.height }} {{ image.mime }} {{ image.href }}"),
            "80x40 image/png page.png"
        );
        assert_eq!(
            rendered("{{ image.data_uri }}"),
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgAB5Sfe/AAAAABJRU5ErkJggg=="
        );
        assert_eq!(
            rendered("{{ data_uri() }}"),
            rendered("{{ image.data_uri }}")
        );
    }

    #[test]
    fn an_image_nobody_supplied_is_there_but_empty() {
        let output = TemplateRenderer
            .render(
                &sample(),
                &ImageSource::new(80, 40),
                &Options::new().with(
                    "template_source",
                    "{% if image.data_uri or image.href %}yes{% else %}no{% endif %}",
                ),
            )
            .unwrap();
        assert_eq!(output.as_str(), Some("no"));
    }

    #[test]
    fn the_layout_is_in_scope_whole() {
        assert_eq!(
            rendered("{{ layout.lines[0].words[0].rotated_box.angle_deg }}"),
            "30.0"
        );
        assert!(rendered("{{ layout | json }}").starts_with(r#"{"version":1,"#));
    }

    #[test]
    fn json_escapes_what_would_end_a_script_early() {
        assert_eq!(
            rendered(r#"{{ "</script>" | json }}"#),
            r#""\u003c/script\u003e""#
        );
    }

    #[test]
    fn markup_is_escaped_only_where_it_is_markup() {
        let source = "{{ vars.text }}";
        let of = |mime: &str| {
            render(
                Options::new()
                    .with("template_source", source)
                    .with("var.text", "a <b> & 'c'")
                    .with("mime", mime),
            )
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
        };
        assert_eq!(of("text/plain"), "a <b> & 'c'");
        assert_eq!(of("text/html"), "a &lt;b&gt; &amp; &#39;c&#39;");
        assert_eq!(of("application/xml"), "a &lt;b&gt; &amp; &#39;c&#39;");
    }

    #[test]
    fn escaping_can_be_asked_for_outright() {
        let of = |choice: &str| {
            render(
                Options::new()
                    .with("template_source", "{{ vars.text }}")
                    .with("var.text", "<b>")
                    .with("mime", "text/html")
                    .with("autoescape", choice),
            )
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
        };
        assert_eq!(of("auto"), "&lt;b&gt;");
        assert_eq!(of("html"), "&lt;b&gt;");
        assert_eq!(of("none"), "<b>");
    }

    #[test]
    fn a_slash_is_left_alone_where_a_browser_would_escape_it() {
        // A `data:` URI is a third slashes; escaping them would treble the
        // size of an embedded image for nothing.
        let rendered = render(
            Options::new()
                .with("template_source", "{{ vars.path }}")
                .with("var.path", "photos/holiday.png")
                .with("mime", "text/html"),
        )
        .unwrap();
        assert_eq!(rendered.as_str(), Some("photos/holiday.png"));
    }

    #[test]
    fn the_escaping_filters_say_which_format_they_are_for() {
        assert_eq!(
            rendered(r#"{{ "it's <b>" | xml_escape }}"#),
            "it&apos;s &lt;b&gt;"
        );
        assert_eq!(
            rendered(r#"{{ "it's <b>" | html_escape }}"#),
            "it&#39;s &lt;b&gt;"
        );
        assert_eq!(rendered(r#"{{ xml_escape("&") }}"#), "&amp;");
    }

    #[test]
    fn an_escaped_value_is_not_escaped_twice() {
        let output = render(
            Options::new()
                .with("template_source", "{{ vars.text | html_escape }}")
                .with("var.text", "<b>")
                .with("mime", "text/html"),
        )
        .unwrap();
        assert_eq!(output.as_str(), Some("&lt;b&gt;"));
    }

    #[test]
    fn numbers_are_written_the_way_a_document_reads_them() {
        assert_eq!(rendered("{{ 28.0 | round }}"), "28");
        assert_eq!(rendered("{{ 28.456 | round(2) }}"), "28.46");
        assert_eq!(rendered("{{ 28.4 | round(3) }}"), "28.4");
        assert_eq!(rendered("{{ round(1.5) }}"), "2");
    }

    #[test]
    fn a_value_of_ones_own_can_be_read_as_a_flag_or_a_number() {
        // A value set on a command line reaches a template as text, and text
        // is true whatever it says unless it is read as a flag.
        assert_eq!(rendered(r#"{{ "false" | flag }}"#), "False");
        assert_eq!(rendered(r#"{{ "off" | flag }}"#), "False");
        assert_eq!(rendered(r#"{{ "yes" | flag }}"#), "True");
        assert_eq!(rendered("{{ true | flag }}"), "True");
        assert_eq!(rendered(r#"{{ "anything" | flag }}"#), "True");
        assert_eq!(rendered(r#"{{ "" | flag }}"#), "False");

        assert_eq!(rendered(r#"{{ ("0.7" | number) * 10 }}"#), "7.0");
        assert_eq!(rendered("{{ 3 | number }}"), "3.0");
        let error =
            render(Options::new().with("template_source", r#"{{ "wide" | number }}"#)).unwrap_err();
        assert!(error.to_string().contains("not a number"), "{error}");
    }

    #[test]
    fn a_box_becomes_a_transform_and_a_polygon() {
        assert_eq!(
            rendered("{{ layout.lines[0].words[0].rotated_box | rotate_transform }}"),
            "rotate(30 18 24)"
        );
        assert_eq!(
            rendered("{{ layout.lines[0].rotated_box | points }}"),
            "10,20 50,20 50,28 10,28"
        );
    }

    #[test]
    fn text_can_be_encoded_as_base64() {
        assert_eq!(rendered(r#"{{ "hello" | base64 }}"#), "aGVsbG8=");
    }

    #[test]
    fn a_template_can_ask_the_svg_renderer_for_a_text_layer() {
        // The document is going inside another one, so it begins at the
        // element rather than at an XML declaration, and it is written as
        // markup rather than escaped into the page as text.
        let layer = render(
            Options::new()
                .with("template_source", "{{ svg(image_mode=\"none\") }}")
                .with("mime", "text/html"),
        )
        .unwrap();
        let layer = layer.as_str().expect("a template writes text");
        assert!(layer.starts_with("<svg "), "{layer}");
        assert!(layer.contains(">hi</tspan>"), "{layer}");
        assert!(!layer.contains("&lt;"), "{layer}");
    }

    #[test]
    fn the_options_of_a_text_layer_can_be_given_either_way() {
        // A mapping is what a template passes the caller's own values
        // through as; keywords are what it settles for itself, and they win.
        let both = rendered(
            r#"{{ svg({"image_mode": "none", "text_mode": "visible"}, text_mode="invisible", ids=true, scope_mode="none") }}"#,
        );
        assert!(both.contains("fill: transparent;"), "{both}");
        assert!(both.contains(r#"id="scribe-line-0""#), "{both}");
        assert_eq!(
            both,
            rendered(r#"{{ {"image_mode": "none", "ids": true, "scope_mode": "none"} | svg }}"#)
        );
    }

    #[test]
    fn the_xml_declaration_of_a_text_layer_can_be_asked_for() {
        let layer = rendered(r#"{{ svg(image_mode="none", xml_declaration=true) }}"#);
        assert!(layer.starts_with("<?xml "), "{layer}");
    }

    #[test]
    fn a_text_layer_refuses_an_option_the_svg_renderer_does_not_take() {
        let error =
            render(Options::new().with("template_source", r#"{{ svg(nonsense=1) }}"#)).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("`nonsense`"), "{message}");
        assert!(message.contains("the svg renderer"), "{message}");
    }

    #[test]
    fn the_options_of_a_text_layer_have_to_be_a_mapping() {
        let error =
            render(Options::new().with("template_source", "{{ svg(\"none\") }}")).unwrap_err();
        assert!(error.to_string().contains("mapping"), "{error}");
    }
}
