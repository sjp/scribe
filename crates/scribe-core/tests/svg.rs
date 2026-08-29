//! What the SVG renderer actually writes.
//!
//! The layout here is the one a recogniser produces for
//! `tests/fixtures/hello.png`: the boxes are the ink in that image, measured
//! from its pixels, so the default snapshot is a document that can be opened
//! in a browser to check that the image is unchanged, that finding "Hello"
//! finds it, and that dragging across a word highlights the word.
//!
//! Only the default snapshot carries the image, since a second copy of it
//! would say nothing new; the rest link to it so that what they are testing
//! stays legible.

use scribe_core::image_source::ImageSource;
use scribe_core::layout::{Layout, Line, Rect, RotatedBox, Word};
use scribe_core::render::{Options, RenderOutput, registry};

/// The image the layout was read from.
const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/hello.png");

/// The size of that image in pixels.
const SIZE: (u32, u32) = (284, 96);

/// A word of the fixture, from the left edge of its ink to the right.
fn word(text: &str, left: f32, right: f32) -> Word {
    let bbox = Rect::new(left, TOP, right - left, BOTTOM - TOP);
    Word {
        text: text.to_string(),
        bbox,
        rotated_box: RotatedBox::from_rect(bbox),
        chars: Vec::new(),
        confidence: Some(0.99),
    }
}

/// The top and bottom of the ink in the fixture.
const TOP: f32 = 31.0;
const BOTTOM: f32 = 63.0;

/// The fixture's one line of text.
fn hello_world() -> Layout {
    let words = vec![word("Hello", 28.0, 126.0), word("World", 143.0, 256.0)];
    let bbox = Rect::new(
        words[0].bbox.x,
        TOP,
        words[1].bbox.right() - words[0].bbox.x,
        BOTTOM - TOP,
    );
    Layout::new(
        SIZE.0,
        SIZE.1,
        vec![Line {
            text: "Hello World".to_string(),
            bbox,
            rotated_box: RotatedBox::from_rect(bbox),
            words,
            confidence: Some(0.99),
        }],
    )
}

/// The same layout with every box turned by `degrees` about the centre of the
/// image, as a photograph of a tilted page would give.
fn turned(mut layout: Layout, degrees: f32) -> Layout {
    let centre = (SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let turn = |box_: &mut RotatedBox| {
        let (x, y) = (box_.cx - centre.0, box_.cy - centre.1);
        *box_ = RotatedBox::new(
            centre.0 + x * cos - y * sin,
            centre.1 + x * sin + y * cos,
            box_.width,
            box_.height,
            box_.angle_deg + degrees,
        );
    };
    for line in &mut layout.lines {
        turn(&mut line.rotated_box);
        line.bbox = line.rotated_box.to_rect();
        for word in &mut line.words {
            turn(&mut word.rotated_box);
            word.bbox = word.rotated_box.to_rect();
        }
    }
    layout
}

/// Renders a layout the way a caller with the fixture to hand would.
fn render(layout: &Layout, options: Options) -> String {
    let registry = registry();
    let renderer = registry.get("svg").expect("svg is built in");
    let image = ImageSource::new(layout.image.width, layout.image.height)
        .with_mime("image/png")
        .with_bytes(FIXTURE)
        .with_href("hello.png");
    let output = renderer
        .render(layout, &image, &options)
        .expect("the fixture layout renders");
    assert_eq!(
        (&*output.mime, &*output.extension),
        ("image/svg+xml", "svg"),
        "an SVG document should say that it is one"
    );
    well_formed(&output)
}

/// The document as text, having checked that it parses as XML.
fn well_formed(output: &RenderOutput) -> String {
    let text = output.as_str().expect("SVG is text").to_string();
    let document = roxmltree::Document::parse(&text)
        .unwrap_or_else(|error| panic!("the document should be well-formed XML: {error}\n{text}"));
    assert_eq!(document.root_element().tag_name().name(), "svg");
    text
}

/// Renders with the image linked to rather than carried, so that a snapshot
/// shows the text layer and not four kilobytes of base64.
fn linked(layout: &Layout, options: Options) -> String {
    render(layout, options.with("image_mode", "link"))
}

#[test]
fn default_output() {
    insta::assert_snapshot!(render(&hello_world(), Options::new()));
}

#[test]
fn visible_text() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new().with("text_mode", "visible")
    ));
}

#[test]
fn debug_text() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new().with("text_mode", "debug")
    ));
}

#[test]
fn linked_image() {
    insta::assert_snapshot!(render(
        &hello_world(),
        Options::new().with("image_mode", "link")
    ));
}

#[test]
fn no_image() {
    insta::assert_snapshot!(render(
        &hello_world(),
        Options::new().with("image_mode", "none")
    ));
}

#[test]
fn rotated_line() {
    insta::assert_snapshot!(linked(&turned(hello_world(), 30.0), Options::new()));
}

#[test]
fn empty_layout() {
    insta::assert_snapshot!(linked(&Layout::empty(SIZE.0, SIZE.1), Options::new()));
}

#[test]
fn every_variant_is_well_formed_xml() {
    // The checking is in `render`; this walks the options that change the
    // shape of the document so that none of them can produce a document no
    // parser will read.
    for (name, value) in [
        ("text_mode", "invisible"),
        ("text_mode", "visible"),
        ("text_mode", "debug"),
        ("image_mode", "link"),
        ("image_mode", "none"),
        ("font_size_scope", "line"),
        ("length_adjust", "spacing"),
        ("class_prefix", "a-"),
        ("title", "A <fixture> & its \"text\""),
        ("aria_label", "A <fixture> & its \"text\""),
    ] {
        linked(&hello_world(), Options::new().with(name, value));
    }
    for (name, value) in [("ids", true), ("include_style", false)] {
        linked(&hello_world(), Options::new().with(name, value));
    }
}
