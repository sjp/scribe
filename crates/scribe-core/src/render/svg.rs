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
//!
//! The document is written to be placed inside another one. Every class name
//! and every id is built from a prefix and a token that sets one document
//! apart from the others in a page, every rule in the stylesheet hangs off
//! the root element rather than reaching the whole of that page, and the
//! layer holds its own against whatever the page styles its text with.
//!
//! How closely the text layer follows the glyphs under it is a caller's
//! choice too: a word can be stretched to fill its box or set out character
//! by character, its baseline can be estimated from what the line says, its
//! size can be read as a cap height, and the spaces between words and the
//! breaks between lines can be given elements of their own so that copying
//! the text out yields what the image says.

use crate::image_source::ImageSource;
use crate::layout::{Char, Line, Rect, RotatedBox};

use super::{
    Layout, OptionKind, OptionSpec, OptionValue, Options, RenderError, RenderOutput, Renderer,
    ResolvedOptions, number,
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
                "font_size_mode",
                OptionKind::Str,
                OptionValue::Str(FontSizeMode::CHOICES[0].to_string()),
                "Whether a box's height is the whole font size or the height of a capital letter set in it, which suits text written in capitals.",
            )
            .with_choices(FontSizeMode::CHOICES),
            OptionSpec::new(
                "cap_height_ratio",
                OptionKind::Float,
                OptionValue::Float(0.7),
                "How much of the font size a capital letter stands, as a fraction, when the size is worked out from cap height.",
            ),
            OptionSpec::new(
                "baseline_mode",
                OptionKind::Str,
                OptionValue::Str(BaselineMode::CHOICES[0].to_string()),
                "Whether the baseline is always the fixed fraction above a box's bottom, or the bottom itself for a line whose characters do not fall below it.",
            )
            .with_choices(BaselineMode::CHOICES),
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
                "char_positions",
                OptionKind::Bool,
                OptionValue::Bool(false),
                "Set each character at the pixels it was read from, where the recogniser said where they are, rather than stretching a whole word to fill its box.",
            ),
            OptionSpec::new(
                "space_mode",
                OptionKind::Str,
                OptionValue::Str(SeparatorMode::CHOICES[0].to_string()),
                "Whether the gap between two words holds a `<tspan>` of its own carrying the space, or the words are parted by a plain space character.",
            )
            .with_choices(SeparatorMode::CHOICES),
            OptionSpec::new(
                "line_break_mode",
                OptionKind::Str,
                OptionValue::Str(SeparatorMode::CHOICES[0].to_string()),
                "Whether one line is parted from the next by a `<tspan>` carrying a newline, so that copying several lines out keeps them on separate lines.",
            )
            .with_choices(SeparatorMode::CHOICES),
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
                "Leave out words the recogniser is less sure of than this, from 0 to 1. The recogniser scribe reads images with reports no confidence, so this filters only a layout that came from somewhere else.",
            ),
            OptionSpec::new(
                "unscored_words",
                OptionKind::Str,
                OptionValue::Str(UnscoredWords::CHOICES[0].to_string()),
                "Whether a word carrying no confidence at all is kept or left out once `min_confidence` is above zero.",
            )
            .with_choices(UnscoredWords::CHOICES),
            OptionSpec::new(
                "text_fill",
                OptionKind::Str,
                OptionValue::Str("#000".to_string()),
                "The colour drawn text is filled with.",
            ),
            OptionSpec::new(
                "selection_fill",
                OptionKind::Str,
                OptionValue::Str("HighlightText".to_string()),
                "The colour selected text shows in; a system colour follows the reader's own.",
            ),
            OptionSpec::new(
                "selection_background",
                OptionKind::Str,
                OptionValue::Str("color-mix(in srgb, Highlight 35%, transparent)".to_string()),
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
                "scope_mode",
                OptionKind::Str,
                OptionValue::Str(ScopeMode::CHOICES[0].to_string()),
                "Whether the class names, the ids and the stylesheet carry a token setting this document apart from anything around it: one worked out from what the document says, one of your own, or none at all.",
            )
            .with_choices(ScopeMode::CHOICES),
            OptionSpec::new(
                "scope",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "The token to set this document apart, when `scope_mode` is `fixed`; a valid CSS identifier part.",
            ),
            OptionSpec::new(
                "ids",
                OptionKind::Bool,
                OptionValue::Bool(false),
                "Give every line and word an id, such as `line-3` and `word-3-1`, under the same prefix and token as the classes.",
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
                "style_nonce",
                OptionKind::Str,
                OptionValue::Str(String::new()),
                "The nonce the stylesheet is written with, for a page whose content security policy does not allow inline styles outright.",
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
        let settings = Settings::read(&options, layout)?;
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

/// Where the token setting one document apart from another comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeMode {
    /// Worked out from what the document says, so that the same layout always
    /// yields the same names and two different layouts do not.
    Content,
    /// The caller's own, which is what two copies of one image in a page need,
    /// since a token worked out from the content would be the same for both.
    Fixed,
    /// None at all, leaving the prefix to carry the whole of a name. This is
    /// the document that stands on its own, with nothing around it to collide
    /// with.
    None,
}

impl ScopeMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["content", "fixed", "none"];

    /// Reads one of [`ScopeMode::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "fixed" => Self::Fixed,
            "none" => Self::None,
            _ => Self::Content,
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

/// What a box's height says about the size of the text in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontSizeMode {
    /// It is the font size itself, so that the tallest letters of a font fill
    /// the box.
    BoxHeight,
    /// It is the height of a capital letter, which is what a box holds when
    /// the text in it has neither ascenders nor descenders.
    CapHeight,
}

impl FontSizeMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["box_height", "cap_height"];

    /// Reads one of [`FontSizeMode::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "cap_height" => Self::CapHeight,
            _ => Self::BoxHeight,
        }
    }
}

/// How far above the bottom of a box its baseline is taken to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BaselineMode {
    /// The fixed fraction of the height the caller gave, whatever the line
    /// says.
    Ratio,
    /// The bottom of the box for a line no character of which falls below the
    /// baseline, and the fixed fraction for one where some character does.
    Estimate,
}

impl BaselineMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["ratio", "estimate"];

    /// Reads one of [`BaselineMode::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "estimate" => Self::Estimate,
            _ => Self::Ratio,
        }
    }
}

/// Whether what parts two pieces of text is an element of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeparatorMode {
    /// It is not, leaving the document as short as it can be.
    None,
    /// It is a `<tspan>`, which stands in the document whether or not a
    /// browser makes anything of the character it holds.
    Tspan,
}

impl SeparatorMode {
    /// The words this mode is chosen by, the first being the default.
    const CHOICES: &'static [&'static str] = &["none", "tspan"];

    /// Reads one of [`SeparatorMode::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "tspan" => Self::Tspan,
            _ => Self::None,
        }
    }
}

/// What becomes of a word the recogniser gave no confidence for, once a
/// threshold is set for the ones it did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnscoredWords {
    /// It is kept: a score nobody gave is not a low one.
    Keep,
    /// It is left out, so that what is left is only what cleared the
    /// threshold.
    Drop,
}

impl UnscoredWords {
    /// The words this choice is made in, the first being the default.
    const CHOICES: &'static [&'static str] = &["keep", "drop"];

    /// Reads one of [`UnscoredWords::CHOICES`], as [`TextMode::read`] does.
    fn read(text: &str) -> Self {
        match text {
            "drop" => Self::Drop,
            _ => Self::Keep,
        }
    }
}

/// The characters taken to fall below the baseline when a baseline is
/// estimated: a rule of thumb for text in the Latin script rather than a fact
/// about any one font.
const DESCENDERS: &str = "gjpqyJQ,;()[]{}/\\|@_$";

/// The ways SVG can stretch a word to the width of its box, the first being
/// the default.
const LENGTH_ADJUST: &[&str] = &["spacingAndGlyphs", "spacing"];

/// A newline written so that it survives being read back out of the document
/// without breaking the line the element is written on.
const NEWLINE: &str = "&#10;";

/// How many characters the token worked out from a layout is written in.
///
/// Six is short enough to read in a name and long enough that two documents
/// in one page are not expected to collide; a caller who knows there are two
/// of one image names them itself.
const TOKEN_LENGTH: usize = 6;

/// The characters a token after its first is written in.
const TOKEN_DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// The starting state of the hash a token is written from.
const HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// What that hash multiplies by as it goes.
const HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The characters a value written into the stylesheet may not hold, each of
/// which could end the declaration, the rule or the element it sits in.
///
/// A value holding none of them means the same thing to the XML parser
/// reading a standalone document and to the HTML parser reading an inlined
/// one, which is what makes escaping the stylesheet unnecessary rather than
/// merely awkward.
const FORBIDDEN_IN_CSS: &[char] = &['<', '>', '&', '{', '}', ';', '@', '\\'];

/// The token that sets a document written from this layout apart from every
/// other, worked out from what the layout says and how large the image is.
///
/// It is derived rather than random so that rendering the same layout twice
/// gives the same document, and it is hashed here rather than through
/// `std::hash::DefaultHasher`, whose output is not promised to be the same
/// from one release of Rust to the next.
pub(crate) fn scope_token(layout: &Layout) -> String {
    let mut hash = HASH_OFFSET;
    hash = hashed(hash, layout.text().as_bytes());
    hash = hashed(hash, &layout.image.width.to_le_bytes());
    hash = hashed(hash, &layout.image.height.to_le_bytes());

    // The first character is a letter, so that the token can begin an
    // identifier when the prefix before it is empty.
    let mut token = String::with_capacity(TOKEN_LENGTH);
    token.push((b'a' + (hash % 26) as u8) as char);
    let mut rest = hash / 26;
    for _ in 1..TOKEN_LENGTH {
        token.push(TOKEN_DIGITS[(rest % TOKEN_DIGITS.len() as u64) as usize] as char);
        rest /= TOKEN_DIGITS.len() as u64;
    }
    token
}

/// One more run of bytes folded into the hash.
fn hashed(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(HASH_PRIME);
    }
    hash
}

/// Checks that a value can be written into a name, rather than escaping it:
/// escaping is what a document does to text, and a name lands in a selector,
/// where the escaped form would name something else.
///
/// # Errors
///
/// Returns an error naming the option when the value holds a character a CSS
/// identifier cannot.
fn check_ident(name: &'static str, value: &str) -> Result<(), RenderError> {
    match value
        .chars()
        .find(|character| !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_')))
    {
        Some(character) => Err(RenderError::unusable_option(
            NAME,
            name,
            value,
            format!("a CSS identifier cannot hold {character:?}"),
        )),
        None => Ok(()),
    }
}

/// Checks that a name a document actually writes begins the way a CSS
/// identifier must: not with a digit, and not with a hyphen followed by one.
///
/// # Errors
///
/// Returns an error against `name`, the option that supplies the start of the
/// composed name.
fn check_ident_start(name: &'static str, value: &str, composed: &str) -> Result<(), RenderError> {
    let mut characters = composed.chars();
    let first = characters.next();
    let starts = match (first, characters.next()) {
        (Some('-'), Some(second)) => !second.is_ascii_digit(),
        (Some('-'), None) => false,
        (Some(first), _) => !first.is_ascii_digit(),
        (None, _) => true,
    };
    if starts {
        return Ok(());
    }
    Err(RenderError::unusable_option(
        NAME,
        name,
        value,
        format!("`{composed}` is not a valid CSS identifier, which cannot begin with a digit"),
    ))
}

/// Checks that a value can stand in a declaration without ending it, so that
/// no colour and no font family can write a rule of its own into a stylesheet
/// that reaches the whole of the page the document is placed in.
///
/// # Errors
///
/// Returns an error naming the option when the value holds a character that
/// could close what it is written into, opens a comment, or leaves a bracket
/// or a quote unclosed.
fn check_css_value(name: &'static str, value: &str) -> Result<(), RenderError> {
    let refuse = |reason: String| RenderError::unusable_option(NAME, name, value, reason);
    if let Some(character) = value
        .chars()
        .find(|character| FORBIDDEN_IN_CSS.contains(character) || (*character as u32) < 0x20)
    {
        return Err(refuse(format!(
            "a value written into a stylesheet cannot hold {character:?}"
        )));
    }
    if value.contains("/*") || value.contains("*/") {
        return Err(refuse(
            "a value written into a stylesheet cannot open or close a comment".to_string(),
        ));
    }
    let mut depth = 0_i32;
    for character in value.chars() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(refuse(
                "a value written into a stylesheet cannot close a bracket it did not open"
                    .to_string(),
            ));
        }
    }
    if depth != 0 {
        return Err(refuse(
            "a value written into a stylesheet has to close every bracket it opens".to_string(),
        ));
    }
    for quote in ['"', '\''] {
        if value.matches(quote).count() % 2 != 0 {
            return Err(refuse(format!(
                "a value written into a stylesheet has to close the {quote} it opens"
            )));
        }
    }
    Ok(())
}

/// Checks that a nonce is one: the base64 a content security policy is
/// written with, and nothing that could end the attribute or the element.
///
/// # Errors
///
/// Returns an error when the value holds anything else.
fn check_nonce(value: &str) -> Result<(), RenderError> {
    match value.chars().find(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=' | '-' | '_'))
    }) {
        Some(character) => Err(RenderError::unusable_option(
            NAME,
            "style_nonce",
            value,
            format!("a nonce is base64, which cannot hold {character:?}"),
        )),
        None => Ok(()),
    }
}

/// The options, read once into the shapes the writing wants them in.
struct Settings<'a> {
    text_mode: TextMode,
    image_mode: ImageMode,
    font_family: &'a str,
    font_size_scope: Scope,
    font_scale: f32,
    font_size_mode: FontSizeMode,
    cap_height_ratio: f32,
    baseline_mode: BaselineMode,
    baseline_ratio: f32,
    length_adjust: &'a str,
    char_positions: bool,
    space_mode: SeparatorMode,
    line_break_mode: SeparatorMode,
    axis_align_tolerance: f32,
    min_confidence: f32,
    unscored_words: UnscoredWords,
    text_fill: &'a str,
    selection_fill: &'a str,
    selection_background: &'a str,
    debug_line_stroke: &'a str,
    debug_word_stroke: &'a str,
    class_prefix: &'a str,
    token: String,
    ids: bool,
    title: &'a str,
    aria_label: &'a str,
    precision: usize,
    include_style: bool,
    style_nonce: &'a str,
    xml_declaration: bool,
}

impl<'a> Settings<'a> {
    /// Reads the options, having refused any of them that cannot be written
    /// into a name, a selector or a declaration.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first option whose value the document
    /// cannot carry.
    fn read(options: &'a ResolvedOptions, layout: &Layout) -> Result<Self, RenderError> {
        let class_prefix = options.str("class_prefix");
        let scope = options.str("scope");
        check_ident("class_prefix", class_prefix)?;
        check_ident("scope", scope)?;
        let token = match ScopeMode::read(options.str("scope_mode")) {
            ScopeMode::Content => scope_token(layout),
            ScopeMode::Fixed if scope.is_empty() => {
                return Err(RenderError::unusable_option(
                    NAME,
                    "scope",
                    scope,
                    "a fixed scope needs a token of its own; \
                     leave `scope_mode` at `content` to have one worked out, \
                     or set it to `none` to go without",
                ));
            }
            ScopeMode::Fixed => scope.to_string(),
            ScopeMode::None => String::new(),
        };
        check_ident_start(
            "class_prefix",
            class_prefix,
            &format!("{class_prefix}{token}"),
        )?;

        let font_family = options.str("font_family");
        check_css_value("font_family", font_family)?;
        for name in [
            "text_fill",
            "selection_fill",
            "selection_background",
            "debug_line_stroke",
            "debug_word_stroke",
        ] {
            check_css_value(name, options.str(name))?;
        }
        let style_nonce = options.str("style_nonce");
        check_nonce(style_nonce)?;

        Ok(Self {
            text_mode: TextMode::read(options.str("text_mode")),
            image_mode: ImageMode::read(options.str("image_mode")),
            font_family,
            font_size_scope: Scope::read(options.str("font_size_scope")),
            font_scale: options.float("font_scale") as f32,
            font_size_mode: FontSizeMode::read(options.str("font_size_mode")),
            cap_height_ratio: options.float("cap_height_ratio") as f32,
            baseline_mode: BaselineMode::read(options.str("baseline_mode")),
            baseline_ratio: options.float("baseline_ratio") as f32,
            length_adjust: options.str("length_adjust"),
            char_positions: options.bool("char_positions"),
            space_mode: SeparatorMode::read(options.str("space_mode")),
            line_break_mode: SeparatorMode::read(options.str("line_break_mode")),
            axis_align_tolerance: options.float("axis_align_tolerance") as f32,
            min_confidence: options.float("min_confidence") as f32,
            unscored_words: UnscoredWords::read(options.str("unscored_words")),
            text_fill: options.str("text_fill"),
            selection_fill: options.str("selection_fill"),
            selection_background: options.str("selection_background"),
            debug_line_stroke: options.str("debug_line_stroke"),
            debug_word_stroke: options.str("debug_word_stroke"),
            class_prefix,
            token,
            ids: options.bool("ids"),
            title: options.str("title"),
            aria_label: options.str("aria_label"),
            precision: options.int("precision").clamp(0, MAX_PRECISION) as usize,
            include_style: options.bool("include_style"),
            style_nonce,
            xml_declaration: options.bool("xml_declaration"),
        })
    }

    /// Whether a box the recogniser scored this well earns a place in the
    /// text layer.
    ///
    /// A box with no score at all is kept, since a score nobody gave says
    /// nothing about the reading, unless a threshold is in force and
    /// `unscored_words` asks for it to be held to that threshold too.
    fn confident_enough(&self, confidence: Option<f32>) -> bool {
        match confidence {
            Some(confidence) => confidence >= self.min_confidence,
            None => self.min_confidence <= 0.0 || self.unscored_words == UnscoredWords::Keep,
        }
    }

    /// A name of the document's own, under the caller's prefix and whatever
    /// token sets this document apart from the others around it.
    fn named(&self, name: &str) -> String {
        if self.token.is_empty() {
            format!("{}{name}", self.class_prefix)
        } else {
            format!("{}{}-{name}", self.class_prefix, self.token)
        }
    }

    /// A class name of the document's own.
    fn class(&self, name: &str) -> String {
        self.named(name)
    }

    /// An id of the document's own. Ids are the names most likely to collide
    /// with a page's own, so they carry the prefix and the token exactly as
    /// the classes do.
    fn id(&self, name: &str) -> String {
        self.named(name)
    }

    /// The id the root element carries, or `None` when no token is in force
    /// and the document claims no name of its own in the page.
    fn root_id(&self) -> Option<String> {
        (!self.token.is_empty()).then(|| format!("{}{}", self.class_prefix, self.token))
    }

    /// What every rule in the stylesheet hangs off: the root element's own id
    /// when it has one, and its class when it does not.
    ///
    /// Either way no rule of this document's is written as a bare class
    /// selector, so that a page holding one cannot have its own elements
    /// styled by it, and so that the specificity the selector gains is enough
    /// to hold the layer against the page's own rules.
    fn root_selector(&self) -> String {
        match self.root_id() {
            Some(id) => format!("#{id}"),
            None => format!(".{}", self.class(ROOT_CLASS)),
        }
    }

    /// A number as the document writes it.
    fn num(&self, value: f32) -> String {
        number(value as f64, self.precision)
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
    if let Some(id) = settings.root_id() {
        root = root.attr("id", &id);
    }
    root = root
        .attr("class", &settings.class(ROOT_CLASS))
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
        let mut style = Tag::new("style");
        if !settings.style_nonce.is_empty() {
            style = style.attr("nonce", settings.style_nonce);
        }
        out.open(&style.open());
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

/// The declarations that make the text layer behave like text however the
/// page around it is styled, written into the stylesheet or, when there is
/// none, into a `style` attribute on the group itself.
///
/// `fill` is among them rather than a presentation attribute because a
/// presentation attribute is the weakest thing in the cascade: a host page
/// saying no more than `svg text { fill: currentColor }` would beat it and
/// turn an invisible layer into a visible one stacked over the picture.
///
/// `all: revert` takes back everything the page said about the group itself,
/// down to the `display` and the `opacity` nothing here names. It cannot take
/// back what the page said further up, since reverting an inherited property
/// leaves the value inherited, so the properties the layer is read and copied
/// by are named as well: a page setting `text-transform` on its `<svg>` would
/// otherwise change what selecting the text yields. What the fitting itself
/// is written in — where a word sits, the size it is set at, the length it is
/// stretched to and the turn of its line — is left alone.
fn layer_declarations(settings: &Settings<'_>) -> String {
    let fill = if settings.text_mode.is_drawn() {
        settings.text_fill
    } else {
        "transparent"
    };
    format!(
        "all: revert; fill: {fill}; stroke: none; font-family: {}; {SETTLED}",
        settings.font_family,
    )
}

/// The properties the layer is laid out, read and copied by, each pinned to
/// what it would be had the page around the document said nothing at all.
const SETTLED: &str = "font-style: normal; font-weight: normal; \
     font-variant: normal; font-stretch: normal; letter-spacing: normal; \
     word-spacing: normal; text-anchor: start; text-decoration: none; \
     text-transform: none; direction: ltr; user-select: text; \
     -webkit-user-select: text; white-space: pre;";

/// The properties a line or a word takes from the group above it rather than
/// from the page around it.
///
/// The group's own declarations are not enough on their own: they reach a
/// `<text>` by inheritance, and a page rule naming `text` outright beats an
/// inherited value. This is the same list under a selector the page cannot
/// outweigh, which is what keeps a layer fitted to its glyphs fitted to them.
const INHERITED: &str = "fill: inherit; stroke: inherit; font-family: inherit; \
     font-style: inherit; font-weight: inherit; font-variant: inherit; \
     font-stretch: inherit; letter-spacing: inherit; word-spacing: inherit; \
     text-anchor: inherit; text-decoration: inherit; text-transform: inherit; \
     direction: inherit; unicode-bidi: isolate; white-space: inherit; \
     user-select: inherit; -webkit-user-select: inherit;";

/// The stylesheet that makes a transparent text layer behave like text.
///
/// Every rule hangs off the root element, which is what keeps a document
/// placed inline in a page from styling anything else there: a `<style>`
/// inside an `<svg>` is not scoped by being inside it, and a bare
/// `.scribe-text` would reach every element of the page carrying that class.
/// The specificity the root selector adds is not a cost but the point, since
/// it is also what holds the layer against the page's own rules.
///
/// The document opts into both colour schemes, so that the system colours the
/// selection is drawn in resolve to whichever one the reader is in, and so
/// that a document opened on its own is drawn on a canvas of the same scheme
/// rather than on white. The opt-in is hung off the root element rather than
/// the document, leaving the colour scheme of a page the document is placed
/// inline in alone.
///
/// Nothing here is escaped, and nothing needs to be: a value that could close
/// a declaration, a rule or the element itself has been refused already.
/// Escaping would be wrong as well as unnecessary, since the XML parser
/// reading a standalone document and the HTML parser reading an inlined one
/// do not agree on what an escape inside a `<style>` means.
fn style_rules(settings: &Settings<'_>) -> Vec<String> {
    let root = settings.root_selector();
    let text = format!("{root} .{}", settings.class(TEXT_CLASS));
    vec![
        format!("{root} {{ color-scheme: light dark; }}"),
        format!("{text} {{ {} }}", layer_declarations(settings)),
        format!("{text} text, {text} tspan {{ {INHERITED} }}"),
        format!(
            "{text}::selection, {text} ::selection {{ fill: {}; background: {}; }}",
            settings.selection_fill, settings.selection_background,
        ),
    ]
}

/// The class the root element carries, and the colour scheme hangs off.
const ROOT_CLASS: &str = "root";

/// The class the text layer's group carries, and the stylesheet hangs off.
const TEXT_CLASS: &str = "text";

/// Writes the group holding one `<text>` per line.
fn write_text_layer(out: &mut Writer, layout: &Layout, settings: &Settings<'_>) {
    let mut group = Tag::new("g").attr("class", &settings.class(TEXT_CLASS));
    // With no stylesheet the same declarations go on the element itself, so
    // that turning the stylesheet off costs the selection colours and nothing
    // else. Those cannot follow it: `::selection` is a pseudo-element, and
    // there is nowhere but a stylesheet to write one.
    if !settings.include_style {
        group = group.attr("style", &layer_declarations(settings));
    }

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
    let last = lines.len() - 1;
    for (position, (index, line, words)) in lines.iter().enumerate() {
        out.line(&text_element(
            *index,
            line,
            words,
            position < last,
            settings,
        ));
    }
    out.close("g");
}

/// Writes one line as a `<text>` element, its words inline so that no
/// indentation of this document's own ends up inside the text.
///
/// The words are separated by single spaces, which is what a browser copies
/// out between them, or by spaces of their own laid over the gaps; a line
/// with another after it may end with a newline, so that copying several
/// lines out keeps them apart.
fn text_element(
    index: usize,
    line: &Line,
    words: &[Placed<'_>],
    followed: bool,
    settings: &Settings<'_>,
) -> String {
    let mut tag = Tag::new("text").attr("class", &settings.class("line"));
    if settings.ids {
        tag = tag.attr("id", &settings.id(&format!("line-{index}")));
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
            &settings.num(font_size(line.rotated_box.height, settings)),
        );
    }

    let mut element = tag.open();
    for (position, word) in words.iter().enumerate() {
        if position > 0 {
            match settings.space_mode {
                SeparatorMode::None => element.push(' '),
                SeparatorMode::Tspan => {
                    element.push_str(&space_tspan(&words[position - 1], word, settings));
                }
            }
        }
        element.push_str(&word.tspan(index, settings));
    }
    if followed && settings.line_break_mode == SeparatorMode::Tspan {
        element.push_str(&break_tspan(settings));
    }
    element.push_str("</text>");
    element
}

/// Writes the space between two words as a `<tspan>` laid over the gap
/// between their boxes, so that every browser copies a space out there and a
/// selection running across the gap is unbroken.
fn space_tspan(before: &Placed<'_>, after: &Placed<'_>, settings: &Settings<'_>) -> String {
    let start = before.x + before.width;
    let gap = (after.x - start).max(0.0);
    let mut tag = Tag::new("tspan")
        .attr("class", &settings.class("space"))
        .attr("x", &settings.num(start))
        .attr("y", &settings.num(before.baseline));
    if settings.font_size_scope == Scope::Word {
        tag = tag.attr(
            "font-size",
            &settings.num(font_size(before.height, settings)),
        );
    }
    if gap > 0.0 {
        tag = tag
            .attr("textLength", &settings.num(gap))
            .attr("lengthAdjust", settings.length_adjust);
    }
    tag.with_text(" ")
}

/// Writes the newline parting one line from the next as a `<tspan>` of its
/// own, which the stylesheet's `white-space: pre` keeps intact.
fn break_tspan(settings: &Settings<'_>) -> String {
    Tag::new("tspan")
        .attr("class", &settings.class("break"))
        .with_raw(NEWLINE)
}

/// The font size text in a box of the given height is set at.
fn font_size(height: f32, settings: &Settings<'_>) -> f32 {
    let size = height * settings.font_scale;
    let ratio = settings.cap_height_ratio;
    match settings.font_size_mode {
        FontSizeMode::CapHeight if ratio.is_finite() && ratio > 0.0 => size / ratio,
        FontSizeMode::CapHeight | FontSizeMode::BoxHeight => size,
    }
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
    /// Where each of its characters starts, when they are to be set one by
    /// one and the recogniser said where they are.
    char_x: Option<Vec<f32>>,
}

impl Placed<'_> {
    /// Writes the word as a `<tspan>` placed at absolute coordinates, so that
    /// selecting or searching it lands on the pixels it was read from.
    fn tspan(&self, line: usize, settings: &Settings<'_>) -> String {
        let mut tag = Tag::new("tspan").attr("class", &settings.class("word"));
        if settings.ids {
            tag = tag.attr("id", &settings.id(&format!("word-{line}-{}", self.index)));
        }
        tag = match &self.char_x {
            Some(starts) => tag.attr(
                "x",
                &starts
                    .iter()
                    .map(|start| settings.num(*start))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            None => tag.attr("x", &settings.num(self.x)),
        };
        tag = tag.attr("y", &settings.num(self.baseline));
        if settings.font_size_scope == Scope::Word {
            tag = tag.attr("font-size", &settings.num(font_size(self.height, settings)));
        }
        // A word set out character by character is already as wide as its
        // box, and stretching it again would undo that.
        if self.char_x.is_none() && self.width > 0.0 {
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
    let baseline_ratio = baseline_ratio(&line.text, settings);

    let nothing: &[Char] = &[];
    let whole_line = [(
        line.text.as_str(),
        line.rotated_box,
        line.confidence,
        nothing,
    )];
    let words = line.words.iter().map(|word| {
        (
            word.text.as_str(),
            word.rotated_box,
            word.confidence,
            word.chars.as_slice(),
        )
    });
    let boxes: Vec<_> = if line.words.is_empty() {
        whole_line.to_vec()
    } else {
        words.collect()
    };

    boxes
        .into_iter()
        .enumerate()
        .filter(|(_, (text, _, confidence, _))| {
            !text.trim().is_empty() && settings.confident_enough(*confidence)
        })
        .map(|(index, (text, box_, _, chars))| {
            let (cx, cy) = rotate_point((box_.cx, box_.cy), -angle, centre);
            Placed {
                index,
                text,
                x: cx - box_.width / 2.0,
                baseline: cy + box_.height * (0.5 - baseline_ratio),
                width: box_.width.max(0.0),
                height: box_.height,
                char_x: settings
                    .char_positions
                    .then(|| char_starts(text, chars, angle, centre))
                    .flatten(),
            }
        })
        .collect()
}

/// How far above the bottom of a line's boxes their baseline sits, as a
/// fraction of their height.
///
/// Estimating it rests on what the line says: a box drawn around text that
/// never dips below the baseline ends at the baseline, and only a line some
/// character of which descends needs room left underneath.
fn baseline_ratio(text: &str, settings: &Settings<'_>) -> f32 {
    let descends = || text.chars().any(|character| DESCENDERS.contains(character));
    match settings.baseline_mode {
        BaselineMode::Ratio => settings.baseline_ratio,
        BaselineMode::Estimate if descends() => settings.baseline_ratio,
        BaselineMode::Estimate => 0.0,
    }
}

/// Where each character of a word starts along its line, or `None` when the
/// recogniser said nothing about the characters or said something that does
/// not line up with the text.
///
/// A character starts at the corner of its box the line reaches first,
/// measured along the direction the line reads in, which for a level line is
/// simply the left edge of the box.
fn char_starts(text: &str, chars: &[Char], angle: f32, centre: (f32, f32)) -> Option<Vec<f32>> {
    if chars.len() != text.chars().count() {
        return None;
    }
    // The characters the document cannot carry are dropped from the text, so
    // their positions have to go with them or every position after would be
    // given to the wrong character.
    let starts: Vec<_> = text
        .chars()
        .zip(chars)
        .filter(|(character, _)| !is_forbidden(*character))
        .map(|(_, character)| projected_start(character.bbox, angle, centre))
        .collect();
    (!starts.is_empty()).then_some(starts)
}

/// Where a box begins along a line reading at `angle`, in the frame that
/// line's rotation is undone in.
fn projected_start(rect: Rect, angle: f32, centre: (f32, f32)) -> f32 {
    let (sin, cos) = angle.to_radians().sin_cos();
    let x = if cos >= 0.0 { rect.x } else { rect.right() };
    let y = if sin >= 0.0 { rect.y } else { rect.bottom() };
    centre.0 + (x - centre.0) * cos + (y - centre.1) * sin
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
            settings
                .ids
                .then(|| settings.id(&format!("line-box-{index}"))),
            settings.debug_line_stroke,
            settings,
        ));
        for (position, word) in line.words.iter().enumerate() {
            out.line(&outline(
                word.rotated_box,
                &settings.class("word-box"),
                settings
                    .ids
                    .then(|| settings.id(&format!("word-box-{index}-{position}"))),
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

    /// The whole of an element holding markup this module has already made
    /// safe to write, such as a character reference.
    fn with_raw(self, markup: &str) -> String {
        format!("{}>{}</{}>", self.text, markup, self.name)
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
    use crate::layout::Word;

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

    /// The same word, with the characters of `text` laid across its box in
    /// the given widths, as a recogniser that says where each one is gives.
    fn lettered(text: &str, x: f32, widths: &[f32]) -> Word {
        let mut word = word(text, x, widths.iter().sum(), Some(0.99));
        let mut left = x;
        word.chars = text
            .chars()
            .zip(widths)
            .map(|(character, width)| {
                let bbox = Rect::new(left, 10.0, *width, 20.0);
                left += width;
                Char {
                    text: character.to_string(),
                    bbox,
                    confidence: None,
                }
            })
            .collect();
        word
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

    /// Renders with no scope token unless the test asks for one, so that what
    /// each of these is about is legible in the name it asserts on. What the
    /// token does is asserted on its own, below.
    fn render_layout(layout: &Layout, options: Options) -> String {
        let mut settings = Options::new().with("scope_mode", "none");
        for (name, value) in options.iter() {
            settings.set(name, value.clone());
        }
        try_render(layout, &settings).expect("the sample layout renders")
    }

    fn try_render(layout: &Layout, options: &Options) -> Result<String, RenderError> {
        let image = ImageSource::new(layout.image.width, layout.image.height)
            .with_mime("image/png")
            .with_bytes(PIXEL_PNG)
            .with_href("scan.png");
        Ok(SvgRenderer
            .render(layout, &image, options)?
            .as_str()
            .expect("SVG is text")
            .to_string())
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
        assert_eq!(
            (&*output.mime, &*output.extension),
            ("image/svg+xml", "svg")
        );
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
        // The colour is a rule rather than a presentation attribute, so that
        // a page styling `text` cannot make an invisible layer visible.
        let hidden = render(Options::new().with("image_mode", "none"));
        assert!(hidden.contains("fill: transparent;"), "{hidden}");
        assert!(!hidden.contains(r#"fill=""#), "{hidden}");

        let visible = render(
            Options::new()
                .with("image_mode", "none")
                .with("text_mode", "visible")
                .with("text_fill", "#123456"),
        );
        assert!(visible.contains("fill: #123456;"), "{visible}");
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
                .contains("transform=\"rotate"),
        );

        layout.lines[0].rotated_box = RotatedBox::new(60.0, 20.0, 100.0, 20.0, 0.6);
        assert!(
            render_layout(&layout, Options::new().with("image_mode", "none"))
                .contains(r#"transform="rotate(0.6 60 20)""#),
        );
    }

    #[test]
    fn characters_can_be_set_where_the_recogniser_saw_them() {
        let mut layout = sample();
        layout.lines[0].words[0] = lettered("Hello", 10.0, &[12.0, 8.0, 5.0, 5.0, 15.0]);
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("char_positions", true),
        );
        // Each character is placed where its own box begins, and the word is
        // no longer stretched over the box as a whole.
        assert!(svg.contains(r#"x="10 22 30 35 40" y="26""#), "{svg}");
        assert!(!svg.contains("textLength=\"45\""), "{svg}");
        // The word beside it, whose characters the recogniser did not give,
        // is stretched as before.
        assert!(
            svg.contains(r#"x="60" y="26" font-size="20" textLength="50""#),
            "{svg}"
        );
    }

    #[test]
    fn characters_of_a_turned_line_are_set_along_the_line() {
        let mut layout = sample();
        layout.lines[0].words[0] = lettered("Hello", 10.0, &[12.0, 8.0, 5.0, 5.0, 15.0]);
        let centre = (60.0, 20.0);
        layout.lines[0].rotated_box = RotatedBox::new(60.0, 20.0, 100.0, 20.0, 30.0);
        for word in &mut layout.lines[0].words {
            let box_ = word.rotated_box;
            let (cx, cy) = rotate_point((box_.cx, box_.cy), 30.0, centre);
            word.rotated_box = RotatedBox::new(cx, cy, box_.width, box_.height, 30.0);
            for character in &mut word.chars {
                let bbox = character.bbox;
                let (cx, cy) = rotate_point(
                    (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0),
                    30.0,
                    centre,
                );
                character.bbox = Rect::new(
                    cx - bbox.width / 2.0,
                    cy - bbox.height / 2.0,
                    bbox.width,
                    bbox.height,
                );
            }
        }

        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("char_positions", true)
                .with("precision", 3_i64),
        );
        let starts: Vec<f32> = svg
            .split_once(r#"<tspan class="scribe-word" x=""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the word carries a position for each character")
            .0
            .split(' ')
            .map(|start| start.parse().expect("a position is a number"))
            .collect();
        assert_eq!(starts.len(), 5, "{svg}");
        // Undoing the line's turn puts the characters back in reading order,
        // each one starting within a pixel of where the one before it ended,
        // which is as close as boxes measured square to the image can say
        // where a turned character begins.
        for (gap, width) in starts
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .zip([12.0, 8.0, 5.0, 5.0])
        {
            assert!((gap - width as f32).abs() < 1.0, "{starts:?}");
        }
    }

    #[test]
    fn a_baseline_can_be_estimated_from_what_the_line_says() {
        let settled = |text: &str| {
            let mut layout = sample();
            layout.lines[0].text = text.to_string();
            render_layout(
                &layout,
                Options::new()
                    .with("image_mode", "none")
                    .with("baseline_mode", "estimate"),
            )
        };

        // Nothing in "Hello World" falls below the baseline, so the bottom of
        // the box is the baseline.
        assert!(
            settled("Hello World").contains(r#"x="10" y="30""#),
            "{}",
            settled("Hello World")
        );
        // "jump" descends, so the box keeps room under it for the tail.
        assert!(
            settled("Hello jump").contains(r#"x="10" y="26""#),
            "{}",
            settled("Hello jump")
        );
    }

    #[test]
    fn a_font_size_can_be_read_as_the_height_of_a_capital() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("font_size_mode", "cap_height"),
        );
        // A capital stands seven tenths of the size it is set at, so filling
        // a box twenty pixels tall with capitals takes a larger size.
        assert!(svg.contains(r#"font-size="28.57""#), "{svg}");

        let told = render(
            Options::new()
                .with("image_mode", "none")
                .with("font_size_mode", "cap_height")
                .with("cap_height_ratio", 0.5),
        );
        assert!(told.contains(r#"font-size="40""#), "{told}");
    }

    #[test]
    fn the_space_between_two_words_can_be_laid_over_the_gap() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("space_mode", "tspan"),
        );
        // The first word ends at 55 and the second begins at 60, so the space
        // is stretched over the five pixels between them.
        assert!(
            svg.contains(
                r#"<tspan class="scribe-space" x="55" y="26" font-size="20" textLength="5" lengthAdjust="spacingAndGlyphs"> </tspan>"#
            ),
            "{svg}"
        );
        assert!(!svg.contains("</tspan> <tspan"), "{svg}");
    }

    #[test]
    fn a_newline_can_part_one_line_from_the_next() {
        let mut layout = sample();
        layout
            .lines
            .push(line(vec![word("Again", 10.0, 45.0, None)]));
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("line_break_mode", "tspan"),
        );
        assert!(
            svg.contains(r#"<tspan class="scribe-break">&#10;</tspan></text>"#),
            "{svg}"
        );
        // Only between the lines: the last one has nothing to be parted from.
        assert_eq!(svg.matches("scribe-break").count(), 1, "{svg}");
        assert!(
            !render_layout(&layout, Options::new().with("image_mode", "none"))
                .contains("scribe-break")
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
    fn words_with_no_confidence_are_kept_when_a_threshold_is_set() {
        // Scribe's own recogniser scores nothing, so a threshold must leave
        // the layouts it wrote alone rather than emptying them.
        let mut layout = sample();
        for word in &mut layout.lines[0].words {
            word.confidence = None;
        }
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("min_confidence", 0.9),
        );
        assert!(svg.contains(">Hello<"), "{svg}");
        assert!(svg.contains(">World<"), "{svg}");
    }

    #[test]
    fn words_with_no_confidence_can_be_left_out_along_with_the_unsure_ones() {
        let mut layout = sample();
        layout.lines[0].words[0].confidence = None;
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("min_confidence", 0.5)
                .with("unscored_words", "drop"),
        );
        assert!(!svg.contains(">Hello<"), "{svg}");
        assert!(!svg.contains(">World<"), "{svg}");
    }

    #[test]
    fn dropping_unscored_words_takes_no_one_without_a_threshold() {
        let mut layout = sample();
        layout.lines[0].words[0].confidence = None;
        let svg = render_layout(
            &layout,
            Options::new()
                .with("image_mode", "none")
                .with("unscored_words", "drop"),
        );
        assert!(svg.contains(">Hello<"), "{svg}");
        assert!(svg.contains(">World<"), "{svg}");
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
            svg.contains(r#"<text class="scribe-line" id="scribe-line-0""#),
            "{svg}"
        );
        assert!(svg.contains(r#"id="scribe-word-0-1""#), "{svg}");
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
    fn selection_is_drawn_in_the_readers_own_colours() {
        let svg = render(Options::new().with("image_mode", "none"));
        assert!(
            svg.contains(".scribe-root { color-scheme: light dark; }"),
            "{svg}"
        );
        assert!(
            svg.contains(r#"<svg xmlns="http://www.w3.org/2000/svg" class="scribe-root""#),
            "{svg}"
        );
        assert!(
            svg.contains(
                "fill: HighlightText; background: color-mix(in srgb, Highlight 35%, transparent);"
            ),
            "{svg}"
        );

        let chosen = render(
            Options::new()
                .with("image_mode", "none")
                .with("selection_fill", "#fff")
                .with("selection_background", "#036"),
        );
        assert!(chosen.contains("fill: #fff; background: #036;"), "{chosen}");
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
        assert!(svg.contains(r#"<g class="scribe-text"/>"#), "{svg}");
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
        assert_eq!(number(f64::NAN, 2), "0");
        assert_eq!(number(f64::INFINITY, 2), "0");

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
    fn a_name_is_worked_out_from_what_the_document_says() {
        let scoped = try_render(&sample(), &Options::new().with("image_mode", "none"))
            .expect("the sample layout renders");
        let token = scope_token(&sample());
        assert_eq!(token.len(), TOKEN_LENGTH, "{token}");
        assert!(
            scoped.contains(&format!(
                r#"id="scribe-{token}" class="scribe-{token}-root""#
            )),
            "{scoped}"
        );
        assert!(
            scoped.contains(&format!(r#"<g class="scribe-{token}-text">"#)),
            "{scoped}"
        );

        // The same layout twice gives the same document, which is what any
        // caching of this output, and every snapshot of it, rests on.
        assert_eq!(
            scoped,
            try_render(&sample(), &Options::new().with("image_mode", "none")).unwrap()
        );

        // A different layout gives a different name, which is the whole point
        // of working one out.
        let mut other = sample();
        other.lines[0].text = "Goodbye World".to_string();
        assert_ne!(scope_token(&other), token);
        assert_ne!(scope_token(&Layout::empty(120, 40)), token);
    }

    #[test]
    fn a_name_of_the_callers_own_stands_in_for_the_worked_out_one() {
        let named = try_render(
            &sample(),
            &Options::new()
                .with("image_mode", "none")
                .with("scope_mode", "fixed")
                .with("scope", "left")
                .with("ids", true),
        )
        .expect("a named scope renders");
        assert!(named.contains(r#"id="scribe-left""#), "{named}");
        assert!(named.contains(r#"id="scribe-left-line-0""#), "{named}");
        assert!(named.contains("#scribe-left .scribe-left-text"), "{named}");

        // Naming the scope and then not saying what it is says nothing.
        let error = try_render(
            &sample(),
            &Options::new()
                .with("image_mode", "none")
                .with("scope_mode", "fixed"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("a fixed scope needs"), "{error}");
    }

    #[test]
    fn every_rule_hangs_off_the_root_rather_than_a_bare_class() {
        for mode in ScopeMode::CHOICES {
            let svg = try_render(
                &sample(),
                &Options::new()
                    .with("image_mode", "none")
                    .with("scope_mode", *mode)
                    .with("scope", "here"),
            )
            .expect("every scope mode renders");
            let rules: Vec<_> = svg
                .lines()
                .map(str::trim)
                .filter(|line| line.ends_with('}'))
                .collect();
            assert_eq!(rules.len(), 4, "{svg}");
            for rule in rules {
                assert!(
                    rule.starts_with('#') || rule.starts_with(".scribe-root "),
                    "{rule}"
                );
            }
        }
    }

    #[test]
    fn a_name_that_is_not_a_css_identifier_is_refused() {
        let refuse = |name: &str, value: &str| {
            try_render(
                &sample(),
                &Options::new().with("image_mode", "none").with(name, value),
            )
            .unwrap_err()
            .to_string()
        };

        assert!(
            refuse("class_prefix", "a b-").contains("cannot hold ' '"),
            "{}",
            refuse("class_prefix", "a b-")
        );
        let composed = refuse("class_prefix", "9-");
        assert!(
            composed.contains("not a valid CSS identifier"),
            "{composed}"
        );

        let error = try_render(
            &sample(),
            &Options::new()
                .with("image_mode", "none")
                .with("scope_mode", "fixed")
                .with("scope", "one two"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("`scope`"), "{error}");
    }

    #[test]
    fn a_colour_that_could_close_its_rule_is_refused() {
        let refuse = |name: &str, value: &str| {
            try_render(
                &sample(),
                &Options::new().with("image_mode", "none").with(name, value),
            )
            .unwrap_err()
            .to_string()
        };

        let closed = refuse("selection_background", "red } * { display: none } .x {");
        assert!(closed.contains("cannot hold '}'"), "{closed}");
        assert!(
            refuse("text_fill", "</style><script>").contains("cannot hold '<'"),
            "{}",
            refuse("text_fill", "</style><script>")
        );
        assert!(
            refuse("font_family", "serif /* out").contains("comment"),
            "{}",
            refuse("font_family", "serif /* out")
        );
        assert!(
            refuse("selection_fill", "rgb(0 0 0").contains("close every bracket"),
            "{}",
            refuse("selection_fill", "rgb(0 0 0")
        );
        assert!(
            refuse("font_family", "\"Times New").contains("close the \""),
            "{}",
            refuse("font_family", "\"Times New")
        );

        // What a colour is actually written as goes through untouched.
        let kept = render(
            Options::new()
                .with("image_mode", "none")
                .with("selection_background", "rgb(0 90 255 / 35%)")
                .with("font_family", "\"Times New Roman\", serif"),
        );
        assert!(kept.contains("background: rgb(0 90 255 / 35%);"), "{kept}");
        assert!(
            kept.contains("font-family: \"Times New Roman\", serif;"),
            "{kept}"
        );
    }

    #[test]
    fn a_stylesheet_can_carry_the_nonce_a_page_demands() {
        let svg = render(
            Options::new()
                .with("image_mode", "none")
                .with("style_nonce", "r4nd0m+t0ken="),
        );
        assert!(svg.contains(r#"<style nonce="r4nd0m+t0ken=">"#), "{svg}");

        let error = try_render(
            &sample(),
            &Options::new()
                .with("image_mode", "none")
                .with("style_nonce", "not a nonce"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("base64"), "{error}");
    }

    #[test]
    fn the_layer_is_still_selectable_with_no_stylesheet() {
        let bare = render(
            Options::new()
                .with("image_mode", "none")
                .with("include_style", false),
        );
        assert!(!bare.contains("<style"), "{bare}");
        let style = bare
            .split_once(r#"<g class="scribe-text" style=""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .expect("the layer carries its own declarations")
            .0;
        for declaration in [
            "fill: transparent;",
            "font-family: sans-serif;",
            "user-select: text;",
            "white-space: pre;",
        ] {
            assert!(style.contains(declaration), "{style}");
        }
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
