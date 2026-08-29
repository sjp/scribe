//! The layout as an SVG document.
//!
//! The document holds the original raster and, over it, one `<text>` element
//! per line carrying one `<tspan>` per word, each placed on the pixels it was
//! recognised from. By default the text is transparent, so the output looks
//! exactly like the image it came from while the browser can select it, search
//! it and read it aloud.
//!
//! Every part of that is an option: the text can be drawn instead of hidden,
//! with the boxes it came from outlined for inspection; the image can be
//! embedded, linked to, or left out so the text layer can be laid over an
//! `<img>` somewhere else; and the font, the classes, the ids, the number of
//! decimals and the stylesheet are all the caller's to choose.

use crate::image_source::ImageSource;
use crate::layout::{Line, RotatedBox};

use super::{
    Layout, OptionKind, OptionSpec, OptionValue, Options, RenderError, RenderOutput, Renderer,
    ResolvedOptions,
};

/// The name this renderer is registered under.
const NAME: &str = "svg";

/// The SVG namespace, without which nothing renders the document as a picture.
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// The namespace of the older `xlink:href`, declared alongside the image so
/// that a document may be post-processed for renderers predating SVG 2.
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// How much of the text a derived `aria-label` carries, in characters. A
/// label is announced in one breath; a whole page of text is not one.
const ARIA_LABEL_CHARS: usize = 200;

/// One level of indentation in the written document.
const INDENT: &str = "  ";

/// The most decimals `precision` can ask for, past which an `f32` has nothing
/// left to say.
const MAX_PRECISION: i64 = 10;

/// Writes the layout as an SVG overlay on the image it came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SvgRenderer;

impl Renderer for SvgRenderer {
    fn name(&self) -> &str {
        NAME
    }

    fn describe_options(&self) -> Vec<OptionSpec> {
        vec![
            OptionSpec::new(
                "text_mode",
                OptionKind::Str,
                OptionValue::Str(TextMode::CHOICES[0].to_string()),
                "Whether the text layer is transparent, drawn over the image, or drawn with the boxes it came from.",
            )
            .with_choices(TextMode::CHOICES),
            OptionSpec::new(
                "image_mode",
                OptionKind::Str,
                OptionValue::Str(ImageMode::CHOICES[0].to_string()),
                "Whether the image is carried in the document, referenced by its path or URL, or left out.",
            )
            .with_choices(ImageMode::CHOICES),
            OptionSpec::new(
                "font_family",
                OptionKind::Str,
                OptionValue::Str("sans-serif".to_string()),
                "The CSS font family the text layer is set in.",
            ),
            OptionSpec::new(
                "font_size_scope",
                OptionKind::Str,
                OptionValue::Str(Scope::CHOICES[0].to_string()),
                "Whether each word takes its font size from its own box or from the line's.",
            )
            .with_choices(Scope::CHOICES),
            OptionSpec::new(
                "font_scale",
                OptionKind::Float,
                OptionValue::Float(1.0),
                "What to multiply a box's height by to get its font size.",
            ),
            OptionSpec::new(
                "baseline_ratio",
                OptionKind::Float,
                OptionValue::Float(0.2),
                "How far above the bottom of a box its baseline sits, as a fraction of the height.",
            ),
            OptionSpec::new(
                "length_adjust",
                OptionKind::Str,
                OptionValue::Str(LENGTH_ADJUST[0].to_string()),
                "Whether fitting a word to its box stretches the gaps alone or the glyphs as well.",
            )
            .with_choices(LENGTH_ADJUST),
            OptionSpec::new(
                "axis_align_tolerance",
                OptionKind::Float,
                OptionValue::Float(0.5),
                "How many degrees off level a line may be before it is given a rotation.",
            ),
            OptionSpec::new(
                "min_confidence",
                OptionKind::Float,
                OptionValue::Float(0.0),
                "Leave out words the recogniser is less sure of than this, from 0 to 1.",
            ),
            OptionSpec::new(
                "text_fill",
                OptionKind::Str,
                OptionValue::Str("#000".to_string()),
                "The colour drawn text is filled with, and that selected text shows in.",
            ),
            OptionSpec::new(
                "selection_background",
                OptionKind::Str,
                OptionValue::Str("rgba(0, 90, 255, 0.35)".to_string()),
                "The colour behind selected text, so that selecting invisible text shows.",
            ),
            OptionSpec::new(
                "debug_line_stroke",
                OptionKind::Str,
                OptionValue::Str("#06c".to_string()),
                "The colour line boxes are outlined in.",
            ),
            OptionSpec::new(
                "debug_word_stroke",
                OptionKind::Str,
                OptionValue::Str("#c00".to_string()),
                "The colour word boxes are outlined in.",
            ),
            OptionSpec::new(
                "class_prefix",
                OptionKind::Str,
                OptionValue::Str("scribe-".to_string()),
                "What every class name in the document starts with; a valid CSS identifier prefix.",
            ),
            OptionSpec::new(
                "ids",
                OptionKind::Bool,
                OptionValue::Bool(false),
                "Give every line and word an id, such as `line-3` and `word-3-1`.",
            ),
            OptionSpec::new(
                "title",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "A title for the document; left out when empty.",
            ),
            OptionSpec::new(
                "aria_label",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "What assistive technology announces; the recognised text when empty.",
            ),
            OptionSpec::new(
                "precision",
                OptionKind::Int,
                OptionValue::Int(2),
                "How many decimals coordinates are written to.",
            ),
            OptionSpec::new(
                "include_style",
                OptionKind::Bool,
                OptionValue::Bool(true),
                "Carry a stylesheet making the text selectable and its selection visible.",
            ),
            OptionSpec::new(
                "xml_declaration",
                OptionKind::Bool,
                OptionValue::Bool(true),
                "Begin the document with an XML declaration.",
            ),
        ]
    }

    fn render(
        &self,
        layout: &Layout,
        image: &ImageSource<'_>,
        options: &Options,
    ) -> Result<RenderOutput, RenderError> {
        let options = self.resolve_options(options)?;
        let settings = Settings::read(&options);
        Ok(RenderOutput::text(
            document(layout, image, &settings)?,
            "image/svg+xml",
            "svg",
        ))
    }
}

/// What the text layer looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextMode {
    /// Transparent, leaving the image as it was.
    Invisible,
    /// Drawn over the image.
    Visible,
    /// Drawn over the image, with the boxes it came from outlined.
    Debug,
}

impl TextMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["invisible", "visible", "debug"];

    /// Reads one of [`TextMode::CHOICES`]. Anything else is the default,
    /// which resolving the options has already ruled out.
    fn read(text: &str) -> Self {
        match text {
            "visible" => Self::Visible,
            "debug" => Self::Debug,
            _ => Self::Invisible,
        }
    }

    /// Whether the text is drawn rather than merely selectable.
    fn is_drawn(self) -> bool {
        matches!(self, Self::Visible | Self::Debug)
    }
}

/// Where the image in the document comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageMode {
    /// Carried in the document as a `data:` URI.
    Embed,
    /// Referenced by the path or URL the caller gave.
    Link,
    /// Left out, leaving a text layer to be laid over the image elsewhere.
    None,
}

impl ImageMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["embed", "link", "none"];

    /// Reads one of [`ImageMode::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "link" => Self::Link,
            "none" => Self::None,
            _ => Self::Embed,
        }
    }
}

/// Which box a word takes its font size from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// Its own.
    Word,
    /// The line's, so that every word of a line is set at one size.
    Line,
}

impl Scope {
    /// The words this scope is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["word", "line"];

    /// Reads one of [`Scope::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "line" => Self::Line,
            _ => Self::Word,
        }
    }
}

/// The ways SVG can stretch a word to the width of its box, the first being
/// the default.
const LENGTH_ADJUST: &[&str] = &["spacingAndGlyphs", "spacing"];

/// The options, read once into the shapes the writing wants them in.
struct Settings<'a> {
    text_mode: TextMode,
    image_mode: ImageMode,
    font_family: &'a str,
    font_size_scope: Scope,
    font_scale: f32,
    baseline_ratio: f32,
    length_adjust: &'a str,
    axis_align_tolerance: f32,
    min_confidence: f32,
    text_fill: &'a str,
    selection_background: &'a str,
    debug_line_stroke: &'a str,
    debug_word_stroke: &'a str,
    class_prefix: &'a str,
    ids: bool,
    title: &'a str,
    aria_label: &'a str,
    precision: usize,
    include_style: bool,
    xml_declaration: bool,
}

impl<'a> Settings<'a> {
    fn read(options: &'a ResolvedOptions) -> Self {
        Self {
            text_mode: TextMode::read(options.str("text_mode")),
            image_mode: ImageMode::read(options.str("image_mode")),
            font_family: options.str("font_family"),
            font_size_scope: Scope::read(options.str("font_size_scope")),
            font_scale: options.float("font_scale") as f32,
            baseline_ratio: options.float("baseline_ratio") as f32,
            length_adjust: options.str("length_adjust"),
            axis_align_tolerance: options.float("axis_align_tolerance") as f32,
            min_confidence: options.float("min_confidence") as f32,
            text_fill: options.str("text_fill"),
            selection_background: options.str("selection_background"),
            debug_line_stroke: options.str("debug_line_stroke"),
            debug_word_stroke: options.str("debug_word_stroke"),
            class_prefix: options.str("class_prefix"),
            ids: options.bool("ids"),
            title: options.str("title"),
            aria_label: options.str("aria_label"),
            precision: options.int("precision").clamp(0, MAX_PRECISION) as usize,
            include_style: options.bool("include_style"),
            xml_declaration: options.bool("xml_declaration"),
        }
    }

    /// A class name of the document's own, under the caller's prefix.
    fn class(&self, name: &str) -> String {
        format!("{}{name}", self.class_prefix)
    }

    /// A number as the document writes it.
    fn num(&self, value: f32) -> String {
        number(value, self.precision)
    }
}

/// Writes the whole document.
fn document(
    layout: &Layout,
    image: &ImageSource<'_>,
    settings: &Settings<'_>,
) -> Result<String, RenderError> {
    let href = image_href(image, settings.image_mode)?;
    let (width, height) = (layout.image.width, layout.image.height);

    let mut out = Writer::new();
    if settings.xml_declaration {
        out.line(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    }

    let mut root = Tag::new("svg").attr("xmlns", SVG_NS);
    if href.is_some() {
        root = root.attr("xmlns:xlink", XLINK_NS);
    }
    root = root
        .attr("width", &width.to_string())
        .attr("height", &height.to_string())
        .attr("viewBox", &format!("0 0 {width} {height}"))
        .attr("role", "img");
    let label = aria_label(layout, settings);
    if !label.is_empty() {
        root = root.attr("aria-label", &label);
    }
    out.open(&root.open());

    if !settings.title.is_empty() {
        out.line(&Tag::new("title").with_text(settings.title));
    }
    if settings.include_style {
        out.open(&Tag::new("style").open());
        for rule in style_rules(settings) {
            out.line(&rule);
        }
        out.close("style");
    }
    if let Some(href) = href {
        out.line(
            &Tag::new("image")
                .attr("href", &href)
                .attr("width", &width.to_string())
                .attr("height", &height.to_string())
                .empty(),
        );
    }

    write_text_layer(&mut out, layout, settings);
    if settings.text_mode == TextMode::Debug {
        write_debug_layer(&mut out, layout, settings);
    }

    out.close("svg");
    Ok(out.finish())
}

/// What the `<image>` element points at, or `None` when the image is left
/// out.
///
/// # Errors
///
/// Returns an error when the caller asked for an image it did not give this
/// renderer enough to produce.
fn image_href(image: &ImageSource<'_>, mode: ImageMode) -> Result<Option<String>, RenderError> {
    match mode {
        ImageMode::Embed => image.data_uri().map(Some).ok_or_else(|| {
            RenderError::missing_image(
                NAME,
                "embed the source image",
                "its bytes and media type are not known",
            )
        }),
        ImageMode::Link => image
            .href
            .map(|href| Some(href.to_string()))
            .ok_or_else(|| {
                RenderError::missing_image(
                    NAME,
                    "link to the source image",
                    "no path or URL for it is known",
                )
            }),
        ImageMode::None => Ok(None),
    }
}

/// The stylesheet that makes a transparent text layer behave like text.
fn style_rules(settings: &Settings<'_>) -> Vec<String> {
    let text = escape(&settings.class(TEXT_CLASS), false);
    vec![
        format!(".{text} {{ user-select: text; -webkit-user-select: text; white-space: pre; }}"),
        format!(
            ".{text}::selection, .{text} ::selection {{ fill: {}; background: {}; }}",
            escape(settings.text_fill, false),
            escape(settings.selection_background, false),
        ),
    ]
}

/// The class the text layer's group carries, and the stylesheet hangs off.
const TEXT_CLASS: &str = "text";

/// Writes the group holding one `<text>` per line.
fn write_text_layer(out: &mut Writer, layout: &Layout, settings: &Settings<'_>) {
    let mut group = Tag::new("g")
        .attr("class", &settings.class(TEXT_CLASS))
        .attr("font-family", settings.font_family);
    group = group.attr(
        "fill",
        if settings.text_mode.is_drawn() {
            settings.text_fill
        } else {
            "transparent"
        },
    );

    let lines: Vec<_> = layout
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line, placed_words(line, settings)))
        .filter(|(_, _, words)| !words.is_empty())
        .collect();
    if lines.is_empty() {
        out.line(&group.empty());
        return;
    }

    out.open(&group.open());
    for (index, line, words) in lines {
        out.line(&text_element(index, line, &words, settings));
    }
    out.close("g");
}

/// Writes one line as a `<text>` element, its words inline so that no
/// indentation of this document's own ends up inside the text.
///
/// The words are separated by single spaces, which is what a browser copies
/// out between them.
fn text_element(
    index: usize,
    line: &Line,
    words: &[Placed<'_>],
    settings: &Settings<'_>,
) -> String {
    let mut tag = Tag::new("text").attr("class", &settings.class("line"));
    if settings.ids {
        tag = tag.attr("id", &format!("line-{index}"));
    }
    let angle = rotation(line.rotated_box, settings);
    if angle != 0.0 {
        tag = tag.attr(
            "transform",
            &format!(
                "rotate({} {} {})",
                settings.num(angle),
                settings.num(line.rotated_box.cx),
                settings.num(line.rotated_box.cy),
            ),
        );
    }
    if settings.font_size_scope == Scope::Line {
        tag = tag.attr(
            "font-size",
            &settings.num(line.rotated_box.height * settings.font_scale),
        );
    }

    let mut element = tag.open();
    for (position, word) in words.iter().enumerate() {
        if position > 0 {
            element.push(' ');
        }
        element.push_str(&word.tspan(index, settings));
    }
    element.push_str("</text>");
    element
}

/// A word laid out in its line's un-rotated frame, ready to be written.
struct Placed<'a> {
    /// Which word of the line this is, for its id.
    index: usize,
    /// The text of the word.
    text: &'a str,
    /// The left edge of its box.
    x: f32,
    /// The baseline the glyphs sit on.
    baseline: f32,
    /// The width of its box, which the glyphs are stretched to fill.
    width: f32,
    /// The height of its box, which its font size comes from.
    height: f32,
}

impl Placed<'_> {
    /// Writes the word as a `<tspan>` placed at absolute coordinates, so that
    /// selecting or searching it lands on the pixels it was read from.
    fn tspan(&self, line: usize, settings: &Settings<'_>) -> String {
        let mut tag = Tag::new("tspan").attr("class", &settings.class("word"));
        if settings.ids {
            tag = tag.attr("id", &format!("word-{line}-{}", self.index));
        }
        tag = tag
            .attr("x", &settings.num(self.x))
            .attr("y", &settings.num(self.baseline));
        if settings.font_size_scope == Scope::Word {
            tag = tag.attr(
                "font-size",
                &settings.num(self.height * settings.font_scale),
            );
        }
        if self.width > 0.0 {
            tag = tag
                .attr("textLength", &settings.num(self.width))
                .attr("lengthAdjust", settings.length_adjust);
        }
        tag.with_text(self.text)
    }
}

/// The words of a line, placed in the frame the line's rotation is undone in.
///
/// Words the recogniser is too unsure of, and words with nothing to show, are
/// left out. A line the recogniser gave no words for is placed whole, so that
/// no recognised text is missing from the overlay.
fn placed_words<'a>(line: &'a Line, settings: &Settings<'_>) -> Vec<Placed<'a>> {
    let angle = rotation(line.rotated_box, settings);
    let centre = (line.rotated_box.cx, line.rotated_box.cy);

    let whole_line = [(line.text.as_str(), line.rotated_box, line.confidence)];
    let words = line
        .words
        .iter()
        .map(|word| (word.text.as_str(), word.rotated_box, word.confidence));
    let boxes: Vec<_> = if line.words.is_empty() {
        whole_line.to_vec()
    } else {
        words.collect()
    };

    boxes
        .into_iter()
        .enumerate()
        .filter(|(_, (text, _, confidence))| {
            !text.trim().is_empty()
                && !confidence.is_some_and(|confidence| confidence < settings.min_confidence)
        })
        .map(|(index, (text, box_, _))| {
            let (cx, cy) = rotate_point((box_.cx, box_.cy), -angle, centre);
            Placed {
                index,
                text,
                x: cx - box_.width / 2.0,
                baseline: cy + box_.height * (0.5 - settings.baseline_ratio),
                width: box_.width.max(0.0),
                height: box_.height,
            }
        })
        .collect()
}

/// Writes the group outlining the boxes the text was read from.
fn write_debug_layer(out: &mut Writer, layout: &Layout, settings: &Settings<'_>) {
    let group = Tag::new("g")
        .attr("class", &settings.class("debug"))
        .attr("fill", "none");
    if layout.lines.is_empty() {
        out.line(&group.empty());
        return;
    }

    out.open(&group.open());
    for (index, line) in layout.lines.iter().enumerate() {
        out.line(&outline(
            line.rotated_box,
            &settings.class("line-box"),
            settings.ids.then(|| format!("line-box-{index}")),
            settings.debug_line_stroke,
            settings,
        ));
        for (position, word) in line.words.iter().enumerate() {
            out.line(&outline(
                word.rotated_box,
                &settings.class("word-box"),
                settings.ids.then(|| format!("word-box-{index}-{position}")),
                settings.debug_word_stroke,
                settings,
            ));
        }
    }
    out.close("g");
}

/// One box outlined in image coordinates: a rectangle when it is level, and
/// the quadrilateral of its corners when it is not.
fn outline(
    box_: RotatedBox,
    class: &str,
    id: Option<String>,
    stroke: &str,
    settings: &Settings<'_>,
) -> String {
    let level = rotation(box_, settings) == 0.0;
    let mut tag = Tag::new(if level { "rect" } else { "polygon" }).attr("class", class);
    if let Some(id) = id {
        tag = tag.attr("id", &id);
    }
    tag = if level {
        tag.attr("x", &settings.num(box_.cx - box_.width / 2.0))
            .attr("y", &settings.num(box_.cy - box_.height / 2.0))
            .attr("width", &settings.num(box_.width.max(0.0)))
            .attr("height", &settings.num(box_.height.max(0.0)))
    } else {
        let points = box_
            .corners()
            .iter()
            .map(|(x, y)| format!("{},{}", settings.num(*x), settings.num(*y)))
            .collect::<Vec<_>>()
            .join(" ");
        tag.attr("points", &points)
    };
    tag.attr("stroke", stroke).empty()
}

/// How far a box is turned, or zero when it is level enough to leave alone.
fn rotation(box_: RotatedBox, settings: &Settings<'_>) -> f32 {
    let angle = box_.angle_deg;
    if !angle.is_finite() || angle.abs() <= settings.axis_align_tolerance.abs() {
        0.0
    } else {
        angle
    }
}

/// Turns a point by `degrees` about `centre`, clockwise as seen on screen.
fn rotate_point(point: (f32, f32), degrees: f32, centre: (f32, f32)) -> (f32, f32) {
    if degrees == 0.0 {
        return point;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (x, y) = (point.0 - centre.0, point.1 - centre.1);
    (centre.0 + x * cos - y * sin, centre.1 + x * sin + y * cos)
}

/// What assistive technology announces the document as: what the caller said,
/// or the recognised text, cut to a length that can be read out.
fn aria_label(layout: &Layout, settings: &Settings<'_>) -> String {
    if !settings.aria_label.is_empty() {
        return settings.aria_label.to_string();
    }
    let mut label = String::new();
    for line in layout.lines.iter().map(|line| line.text.trim()) {
        if line.is_empty() {
            continue;
        }
        if !label.is_empty() {
            label.push(' ');
        }
        label.push_str(line);
        if label.chars().count() >= ARIA_LABEL_CHARS {
            break;
        }
    }
    label.chars().take(ARIA_LABEL_CHARS).collect()
}

/// Writes a number with at most `precision` decimals, dropping the trailing
/// zeros so that whole pixels read as whole numbers.
///
/// A number SVG cannot hold is written as zero rather than as `NaN`, which no
/// renderer would accept.
fn number(value: f32, precision: usize) -> String {
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

/// Replaces the characters XML gives a meaning of its own, and drops the ones
/// it cannot carry at all.
///
/// Tabs and newlines become spaces, since they mean nothing to a word of
/// recognised text and everything to a reader of the document.
fn escape(text: &str, in_attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if in_attribute => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            character if is_forbidden(character) => {}
            character => out.push(character),
        }
    }
    out
}

/// Whether XML has no way of carrying a character: the control characters and
/// the two code points that are never characters at all.
fn is_forbidden(character: char) -> bool {
    character < '\u{20}'
        || ('\u{7f}'..='\u{9f}').contains(&character)
        || matches!(character, '\u{fffe}' | '\u{ffff}')
}

/// One start tag, with every attribute value escaped as it is added.
struct Tag<'a> {
    name: &'a str,
    text: String,
}

impl<'a> Tag<'a> {
    /// Begins a tag of the given element name.
    fn new(name: &'a str) -> Self {
        Self {
            name,
            text: format!("<{name}"),
        }
    }

    /// Adds one attribute.
    #[must_use]
    fn attr(mut self, name: &str, value: &str) -> Self {
        self.text.push(' ');
        self.text.push_str(name);
        self.text.push_str("=\"");
        self.text.push_str(&escape(value, true));
        self.text.push('"');
        self
    }

    /// The start tag of an element that has children.
    fn open(self) -> String {
        self.text + ">"
    }

    /// The whole of an element with nothing in it.
    fn empty(self) -> String {
        self.text + "/>"
    }

    /// The whole of an element holding only text.
    fn with_text(self, text: &str) -> String {
        format!("{}>{}</{}>", self.text, escape(text, false), self.name)
    }
}

/// The document as it is built, a line at a time, indented by how deep in it
/// each line sits.
struct Writer {
    out: String,
    depth: usize,
}

impl Writer {
    fn new() -> Self {
        Self {
            out: String::new(),
            depth: 0,
        }
    }

    /// Writes one line at the current depth.
    fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.out.push_str(INDENT);
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Writes a start tag and indents what follows it.
    fn open(&mut self, tag: &str) {
        self.line(tag);
        self.depth += 1;
    }

    /// Closes the element [`Writer::open`] last opened.
    fn close(&mut self, name: &str) {
        self.depth = self.depth.saturating_sub(1);
        self.line(&format!("</{name}>"));
    }

    fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Rect, Word};

    /// The smallest PNG that decoders accept: one opaque black pixel.
    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a,
        0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn word(text: &str, x: f32, width: f32, confidence: Option<f32>) -> Word {
        Word {
            text: text.to_string(),
            bbox: Rect::new(x, 10.0, width, 20.0),
            rotated_box: RotatedBox::new(x + width / 2.0, 20.0, width, 20.0, 0.0),
            chars: Vec::new(),
            confidence,
        }
    }

    fn line(words: Vec<Word>) -> Line {
        let text = words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Line {
            text,
            bbox: Rect::new(10.0, 10.0, 100.0, 20.0),
            rotated_box: RotatedBox::new(60.0, 20.0, 100.0, 20.0, 0.0),
            words,
            confidence: None,
        }
    }

    fn sample() -> Layout {
        Layout::new(
            120,
            40,
            vec![line(vec![
                word("Hello", 10.0, 45.0, Some(0.98)),
                word("World", 60.0, 50.0, Some(0.42)),
            ])],
        )
    }

    fn render(options: Options) -> String {
        render_layout(&sample(), options)
    }

    fn render_layout(layout: &Layout, options: Options) -> String {
        let image = ImageSource::new(layout.image.width, layout.image.height)
            .with_mime("image/png")
            .with_bytes(PIXEL_PNG)
            .with_href("scan.png");
        SvgRenderer
            .render(layout, &image, &options)
            .expect("the sample layout renders")
            .as_str()
            .expect("SVG is text")
            .to_string()
    }

    #[test]
    fn the_output_says_what_it_is() {
        let output = SvgRenderer
            .render(
                &sample(),
                &ImageSource::new(120, 40),
                &Options::new().with("image_mode", "none"),
            )
            .unwrap();
        assert_eq!((output.mime, output.extension), ("image/svg+xml", "svg"));
    }

    #[test]
    fn words_are_placed_at_the_pixels_they_were_read_from() {
        let svg = render(Options::new().with("image_mode", "none"));
        // The box is 20 high from y=10, so the baseline sits a fifth of that
        // above its bottom, and the glyphs are stretched over its width.
        assert!(
            svg.contains(
                r#"<tspan class="scribe-word" x="10" y="26" font-size="20" textLength="45" lengthAdjust="spacingAndGlyphs">Hello</tspan>"#
            ),
            "{svg}"
        );
    }

    #[test]
    fn words_are_separated_by_the_space_a_browser_copies_between_them() {
        let svg = render(Options::new().with("image_mode", "none"));
        assert!(svg.contains("</tspan> <tspan"), "{svg}");
    }

    #[test]
    fn the_text_is_transparent_until_it_is_asked_for() {
        assert!(
            render(Options::new().with("image_mode", "none")).contains(r#"fill="transparent""#)
        );

        let visible = render(
            Options::new()
                .with("image_mode", "none")
                .with("text_mode", "visible")
                .with("text_fill", "#123456"),
        );
        assert!(visible.contains(r##"fill="#123456""##), "{visible}");
        assert!(!visible.contains("scribe-debug"), "{visible}");
    }

    #[test]
    fn debug_mode_outlines_the_boxes_the_text_came_from() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("text_mode", "debug"),
        );
        assert!(
            svg.contains(r#"<g class="scribe-debug" fill="none">"#),
            "{svg}"
        );
        assert!(
            svg.contains(
                r##"<rect class="scribe-line-box" x="10" y="10" width="100" height="20" stroke="#06c"/>"##
            ),
            "{svg}"
        );
        assert!(svg.contains(r#"class="scribe-word-box""#), "{svg}");
    }

    #[test]
    fn a_rotated_line_carries_its_turn_and_holds_its_words_level() {
        let mut layout = sample();
        layout.lines[0].rotated_box = RotatedBox::new(60.0, 20.0, 100.0, 20.0, 30.0);
        for word in &mut layout.lines[0].words {
            let box_ = word.rotated_box;
            let (cx, cy) = rotate_point((box_.cx, box_.cy), 30.0, (60.0, 20.0));
            word.rotated_box = RotatedBox::new(cx, cy, box_.width, box_.height, 30.0);
        }

        let svg = render_layout(&layout, Options::new().with("image_mode", "none"));
        assert!(svg.contains(r#"transform="rotate(30 60 20)""#), "{svg}");
        // Undoing the line's rotation puts the words back where they would
        // have been had the line been level.
        assert!(svg.contains(r#"x="10" y="26""#), "{svg}");
    }

    #[test]
    fn a_line_barely_off_level_is_left_level() {
        let mut layout = sample();
        layout.lines[0].rotated_box = RotatedBox::new(60.0, 20.0, 100.0, 20.0, 0.4);
        assert!(
            !render_layout(&layout, Options::new().with("image_mode", "none"))
                .contains("transform"),
        );

        layout.lines[0].rotated_box = RotatedBox::new(60.0, 20.0, 100.0, 20.0, 0.6);
        assert!(
            render_layout(&layout, Options::new().with("image_mode", "none"))
                .contains(r#"transform="rotate(0.6 60 20)""#),
        );
    }

    #[test]
    fn the_font_size_can_come_from_the_line_instead_of_the_word() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("font_size_scope", "line")
                .with("font_scale", 0.9),
        );
        assert!(
            svg.contains(r#"<text class="scribe-line" font-size="18">"#),
            "{svg}"
        );
        assert!(
            !svg.contains("<tspan class=\"scribe-word\" x=\"10\" y=\"26\" font-size"),
            "{svg}"
        );
    }

    #[test]
    fn words_the_recogniser_is_unsure_of_can_be_left_out() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("min_confidence", 0.5),
        );
        assert!(svg.contains(">Hello<"), "{svg}");
        assert!(!svg.contains(">World<"), "{svg}");
    }

    #[test]
    fn a_line_with_no_words_is_placed_whole() {
        let mut layout = sample();
        layout.lines[0].words.clear();
        let svg = render_layout(&layout, Options::new().with("image_mode", "none"));
        assert!(svg.contains(">Hello World</tspan>"), "{svg}");
    }

    #[test]
    fn ids_name_each_line_and_word_by_its_place_in_the_layout() {
        let svg = render(Options::new().with("image_mode", "none").with("ids", true));
        assert!(
            svg.contains(r#"<text class="scribe-line" id="line-0""#),
            "{svg}"
        );
        assert!(svg.contains(r#"id="word-0-1""#), "{svg}");
    }

    #[test]
    fn classes_and_the_stylesheet_follow_the_chosen_prefix() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("class_prefix", "ocr-"),
        );
        assert!(svg.contains(r#"<g class="ocr-text""#), "{svg}");
        assert!(svg.contains(".ocr-text ::selection"), "{svg}");
        assert!(!svg.contains("scribe-"), "{svg}");
    }

    #[test]
    fn the_stylesheet_and_the_declaration_can_be_left_out() {
        let bare = render(
            Options::new()
                .with("image_mode", "none")
                .with("include_style", false)
                .with("xml_declaration", false),
        );
        assert!(!bare.contains("<style>"), "{bare}");
        assert!(bare.starts_with("<svg "), "{bare}");
    }

    #[test]
    fn the_label_is_the_text_until_the_caller_says_otherwise() {
        let svg = render(Options::new().with("image_mode", "none"));
        assert!(svg.contains(r#"aria-label="Hello World""#), "{svg}");

        let said = render(
            Options::new()
                .with("image_mode", "none")
                .with("aria_label", "A scanned receipt")
                .with("title", "Receipt"),
        );
        assert!(said.contains(r#"aria-label="A scanned receipt""#), "{said}");
        assert!(said.contains("<title>Receipt</title>"), "{said}");
    }

    #[test]
    fn a_long_text_is_cut_to_a_label_that_can_be_read_out() {
        let mut layout = sample();
        layout.lines[0].text = "word ".repeat(100);
        let svg = render_layout(&layout, Options::new().with("image_mode", "none"));
        let label = svg
            .split_once(r#"aria-label=""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the document has a label")
            .0;
        assert_eq!(label.chars().count(), ARIA_LABEL_CHARS);
    }

    #[test]
    fn a_layout_with_no_text_still_reproduces_the_image() {
        let svg = render_layout(&Layout::empty(4, 2), Options::new());
        assert!(
            svg.contains(r#"<image href="data:image/png;base64,"#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"<g class="scribe-text" font-family="sans-serif" fill="transparent"/>"#),
            "{svg}"
        );
        assert!(!svg.contains("aria-label"), "{svg}");
    }

    #[test]
    fn the_image_can_be_linked_to_instead_of_carried() {
        let svg = render(Options::new().with("image_mode", "link"));
        assert!(
            svg.contains(r#"<image href="scan.png" width="120" height="40"/>"#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"xmlns:xlink="http://www.w3.org/1999/xlink""#),
            "{svg}"
        );
    }

    #[test]
    fn leaving_the_image_out_leaves_out_its_namespace_too() {
        let svg = render(Options::new().with("image_mode", "none"));
        assert!(!svg.contains("<image"), "{svg}");
        assert!(!svg.contains("xlink"), "{svg}");
    }

    #[test]
    fn an_image_the_caller_did_not_provide_is_an_error_rather_than_a_blank() {
        let error = SvgRenderer
            .render(&sample(), &ImageSource::new(120, 40), &Options::new())
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "the svg renderer was asked to embed the source image, \
             but its bytes and media type are not known"
        );

        let error = SvgRenderer
            .render(
                &sample(),
                &ImageSource::new(120, 40),
                &Options::new().with("image_mode", "link"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("no path or URL for it is known"));
    }

    #[test]
    fn an_option_that_takes_one_of_a_few_words_rejects_any_other() {
        let error = SvgRenderer
            .render(
                &sample(),
                &ImageSource::new(120, 40),
                &Options::new().with("text_mode", "faint"),
            )
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "the svg renderer's `text_mode` option takes one of \
             `invisible`, `visible`, `debug`, but was given \"faint\""
        );
    }

    #[test]
    fn text_that_would_break_the_document_is_escaped_or_dropped() {
        let mut layout = sample();
        layout.lines[0].words = vec![word("<a & b>\u{7}", 10.0, 45.0, None)];
        layout.lines[0].text = "\"quoted\"\ttabbed".to_string();
        let svg = render_layout(&layout, Options::new().with("image_mode", "none"));
        assert!(svg.contains(">&lt;a &amp; b&gt;</tspan>"), "{svg}");
        assert!(
            svg.contains(r#"aria-label="&quot;quoted&quot; tabbed""#),
            "{svg}"
        );
        assert!(!svg.contains('\u{7}'), "{svg}");
    }

    #[test]
    fn coordinates_are_written_to_the_asked_for_number_of_decimals() {
        assert_eq!(number(12.0, 2), "12");
        assert_eq!(number(12.345, 2), "12.35");
        assert_eq!(number(12.345, 0), "12");
        assert_eq!(number(-0.001, 2), "0");
        assert_eq!(number(f32::NAN, 2), "0");
        assert_eq!(number(f32::INFINITY, 2), "0");

        let mut layout = sample();
        layout.lines[0].words = vec![word("Hello", 10.125, 45.0, None)];
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("precision", 3_i64),
        );
        assert!(svg.contains(r#"x="10.125""#), "{svg}");
    }

    #[test]
    fn every_option_is_described() {
        let names: Vec<_> = SvgRenderer
            .describe_options()
            .into_iter()
            .map(|spec| spec.name)
            .collect();
        assert!(names.contains(&"text_mode"), "{names:?}");
        assert!(
            SvgRenderer
                .describe_options()
                .iter()
                .all(|spec| !spec.help.is_empty())
        );
    }
}
