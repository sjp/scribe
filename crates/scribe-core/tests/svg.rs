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
fn scoped_names() {
    insta::assert_snapshot!(linked(&hello_world(), Options::new().with("ids", true)));
}

#[test]
fn fixed_scope() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new()
            .with("scope_mode", "fixed")
            .with("scope", "second")
            .with("ids", true)
    ));
}

#[test]
fn unscoped_names() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new().with("scope_mode", "none").with("ids", true)
    ));
}

#[test]
fn style_nonce() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new().with("style_nonce", "rAnd0m/nonce+FromTheServer=")
    ));
}

#[test]
fn no_stylesheet() {
    insta::assert_snapshot!(linked(
        &hello_world(),
        Options::new().with("include_style", false)
    ));
}

#[test]
fn the_older_way_of_pointing_at_an_image_is_a_namespace_a_reader_can_resolve() {
    // A prefix used without being declared is not well-formed XML at all, so
    // parsing this is the whole of the check that the declaration is there.
    let text = linked(&hello_world(), Options::new().with("xlink", true));
    let document = roxmltree::Document::parse(&text)
        .unwrap_or_else(|error| panic!("the document should be well-formed XML: {error}\n{text}"));
    let image = document
        .descendants()
        .find(|node| node.has_tag_name("image"))
        .expect("the image is written");
    assert_eq!(
        image.attribute(("http://www.w3.org/1999/xlink", "href")),
        Some("hello.png"),
        "{text}"
    );
    assert_eq!(image.attribute("href"), Some("hello.png"), "{text}");
}

#[test]
fn the_same_layout_and_options_give_the_same_bytes() {
    // Every test here is a snapshot, and a caller may well be keeping this
    // output beside the image it came from; both need the document to be a
    // function of the layout and the options and of nothing else.
    for options in [
        Options::new(),
        Options::new().with("ids", true).with("text_mode", "debug"),
        Options::new().with("scope_mode", "none"),
    ] {
        let once = linked(&hello_world(), options.clone());
        assert_eq!(once, linked(&hello_world(), options));
    }
}

#[test]
fn two_layouts_in_one_page_share_no_name() {
    let scoped = |layout: &Layout| {
        let text = linked(layout, Options::new().with("ids", true));
        let document = roxmltree::Document::parse(&text).expect("the document parses");
        let names: Vec<String> = document
            .descendants()
            .filter_map(|node| node.attribute("id"))
            .chain(
                document
                    .descendants()
                    .filter_map(|node| node.attribute("class")),
            )
            .map(str::to_string)
            .collect();
        assert!(!names.is_empty(), "{text}");
        names
    };

    let one = scoped(&hello_world());
    let other = scoped(&turned(other_layout(), 0.0));
    for name in &one {
        assert!(!other.contains(name), "both documents write `{name}`");
    }
}

/// A second layout, of a different size and saying something else, standing
/// for the other picture on a page this one is placed in.
fn other_layout() -> Layout {
    let words = vec![word("Second", 20.0, 140.0), word("page", 150.0, 240.0)];
    let bbox = Rect::new(
        words[0].bbox.x,
        TOP,
        words[1].bbox.right() - words[0].bbox.x,
        BOTTOM - TOP,
    );
    Layout::new(
        SIZE.0 + 10,
        SIZE.1,
        vec![Line {
            text: "Second page".to_string(),
            bbox,
            rotated_box: RotatedBox::from_rect(bbox),
            words,
            confidence: Some(0.99),
        }],
    )
}

#[test]
fn no_rule_reaches_past_the_document_it_is_written_in() {
    // A `<style>` inside an inline `<svg>` styles the whole of the page
    // around it, so a rule that begins with a bare class would reach every
    // element there carrying that class.
    let text = render(&hello_world(), Options::new().with("image_mode", "none"));
    let document = roxmltree::Document::parse(&text).expect("the document parses");
    let stylesheet = document
        .descendants()
        .find(|node| node.has_tag_name("style"))
        .and_then(|node| node.text())
        .expect("the document carries a stylesheet");
    let rules: Vec<_> = stylesheet
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(!rules.is_empty(), "{stylesheet}");
    for rule in rules {
        assert!(
            rule.starts_with(&format!("#{}", root_id(&text))),
            "every rule should hang off the root element, but one is `{rule}`"
        );
    }
}

/// The id the root element carries, which every rule is written under.
fn root_id(text: &str) -> String {
    let document = roxmltree::Document::parse(text).expect("the document parses");
    document
        .root_element()
        .attribute("id")
        .expect("the root element is named")
        .to_string()
}

#[test]
fn a_layer_with_no_stylesheet_is_still_text() {
    let text = render(
        &hello_world(),
        Options::new()
            .with("image_mode", "none")
            .with("include_style", false),
    );
    assert!(!text.contains("<style"), "{text}");
    let document = roxmltree::Document::parse(&text).expect("the document parses");
    let group = document
        .descendants()
        .find(|node| node.has_tag_name("g"))
        .expect("the text layer is written");
    let style = group
        .attribute("style")
        .expect("the layer carries its own declarations");
    for declaration in ["fill: transparent", "user-select: text", "white-space: pre"] {
        assert!(style.contains(declaration), "{style}");
    }
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
        ("font_size_mode", "cap_height"),
        ("baseline_mode", "estimate"),
        ("length_adjust", "spacing"),
        ("space_mode", "tspan"),
        ("line_break_mode", "tspan"),
        ("class_prefix", "a-"),
        ("scope_mode", "none"),
        ("style_nonce", "n0nce="),
        ("title", "A <fixture> & its \"text\""),
        ("aria_label", "A <fixture> & its \"text\""),
    ] {
        linked(&hello_world(), Options::new().with(name, value));
    }
    for (name, value) in [
        ("ids", true),
        ("include_style", false),
        ("char_positions", true),
    ] {
        linked(&hello_world(), Options::new().with(name, value));
    }
}
