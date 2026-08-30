//! What every built-in renderer makes of the fixture layouts.
//!
//! The layouts in `tests/fixtures` are what the pipeline read from the images
//! beside them, so these are end-to-end results without an end-to-end run:
//! nothing here loads a model, and a change in any renderer shows up as a
//! change in a snapshot. `UPDATE_FIXTURES=1`, with the model paths in the
//! environment, draws the images and reads the layouts again.

mod support;

use scribe_core::image_source::ImageSource;
use scribe_core::layout::{Layout, Rect};
use scribe_core::render::{Options, list_templates, registry};

use support::{FIXTURES, Fixture};

/// How far outside its line a word's box may reach and still count as being
/// inside it: the rounding of two boxes measured separately, and no more.
const SLACK: f32 = 1.0;

/// The fixtures the refinements to how text is fitted are snapshotted on: one
/// page of level text in three sizes, and one of text that is turned.
const FITTED: &[&str] = &["paragraph", "rotated"];

/// Each way of fitting the text layer more closely to the glyphs under it,
/// named for its snapshot and set on top of the defaults.
const FITTINGS: &[(&str, &str, &str)] = &[
    ("char-positions", "char_positions", "true"),
    ("baseline-estimate", "baseline_mode", "estimate"),
    ("cap-height", "font_size_mode", "cap_height"),
    ("space-tspan", "space_mode", "tspan"),
    ("line-break-tspan", "line_break_mode", "tspan"),
];

/// Renders a fixture's layout the way a caller with the image to hand would.
fn render(fixture: &Fixture, name: &str, options: Options) -> String {
    render_image(fixture, name, options, true)
}

/// Renders a fixture's layout for a caller who has only a path to the image,
/// so that a snapshot shows the text layer and not the image all over again.
fn render_linked(fixture: &Fixture, name: &str, options: Options) -> String {
    render_image(fixture, name, options, false)
}

/// Renders a fixture's layout, with the image itself either to hand or only
/// named.
fn render_image(fixture: &Fixture, name: &str, options: Options, carried: bool) -> String {
    let registry = registry();
    let renderer = registry
        .get(name)
        .unwrap_or_else(|| panic!("`{name}` is built in"));
    let layout = fixture.layout();
    let bytes = fixture.image_bytes();
    let file_name = fixture.file_name();
    let mut image = ImageSource::new(layout.image.width, layout.image.height)
        .with_mime(fixture.kind.mime())
        .with_href(&file_name);
    if carried {
        image = image.with_bytes(&bytes);
    }
    let output = renderer
        .render(&layout, &image, &options)
        .unwrap_or_else(|error| panic!("{file_name} renders as {name}: {error}"));
    output
        .as_str()
        .unwrap_or_else(|| panic!("{name} writes text"))
        .to_string()
}

#[test]
fn every_layout_describes_the_image_beside_it() {
    support::prepare();
    for fixture in FIXTURES {
        let layout = fixture.layout();
        let image = image::load_from_memory(&fixture.image_bytes())
            .unwrap_or_else(|error| panic!("{} cannot be decoded: {error}", fixture.file_name()));
        assert_eq!(
            (layout.image.width, layout.image.height),
            (image.width(), image.height()),
            "{} was read from an image of another size",
            fixture.file_name()
        );
        assert_eq!(
            (layout.image.width, layout.image.height),
            (fixture.width, fixture.height),
            "{} is not the size it is drawn at",
            fixture.file_name()
        );
    }
}

#[test]
fn the_blank_fixture_has_nothing_to_read() {
    support::prepare();
    let blank = FIXTURES
        .iter()
        .find(|fixture| fixture.stem == "blank")
        .expect("the set has a blank fixture");
    assert_eq!(blank.layout(), Layout::empty(blank.width, blank.height));
}

#[test]
fn words_sit_inside_their_lines_and_every_box_inside_the_image() {
    support::prepare();
    let mut checked = 0;
    for fixture in FIXTURES {
        let layout = fixture.layout();
        let name = fixture.file_name();
        let (width, height) = (layout.image.width as f32, layout.image.height as f32);
        let inside_image = |corners: [(f32, f32); 4]| {
            corners
                .iter()
                .all(|(x, y)| *x >= 0.0 && *y >= 0.0 && *x <= width && *y <= height)
        };

        for line in &layout.lines {
            assert!(
                inside_image(line.rotated_box.corners()),
                "{name}: the line {:?} is at {:?}, which leaves a {width} by {height} image",
                line.text,
                line.rotated_box.corners()
            );
            for word in &line.words {
                assert!(
                    within(word.bbox, line.bbox),
                    "{name}: the word {:?} at {:?} is outside its line {:?} at {:?}",
                    word.text,
                    word.bbox,
                    line.text,
                    line.bbox
                );
                assert!(
                    inside_image(word.rotated_box.corners()),
                    "{name}: the word {:?} is at {:?}, which leaves a {width} by {height} image",
                    word.text,
                    word.rotated_box.corners()
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "the fixtures should hold words to check, or this promises nothing"
    );
}

/// Whether one rectangle is inside another, allowing for [`SLACK`].
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.right() <= outer.right() + SLACK
        && inner.bottom() <= outer.bottom() + SLACK
}

#[test]
fn json_output() {
    support::prepare();
    for fixture in FIXTURES {
        let output = render(fixture, "json", Options::new());
        assert!(
            Layout::from_json(&output).is_ok(),
            "the json renderer should write a layout, but wrote {output}"
        );
        insta::assert_snapshot!(format!("json-{}", fixture.stem), output);
    }
}

#[test]
fn svg_output() {
    support::prepare();
    for fixture in FIXTURES {
        let output = render(fixture, "svg", Options::new());
        let document = roxmltree::Document::parse(&output).unwrap_or_else(|error| {
            panic!(
                "{}: the SVG should be well-formed XML: {error}",
                fixture.stem
            )
        });
        assert_eq!(document.root_element().tag_name().name(), "svg");
        insta::assert_snapshot!(format!("svg-{}", fixture.stem), output);
    }
}

#[test]
fn svg_fitting_options() {
    support::prepare();
    for stem in FITTED {
        let fixture = fixture(stem);
        for (snapshot, name, value) in FITTINGS {
            // Linked rather than embedded, so that a snapshot shows the text
            // layer and not the image all over again.
            let options = Options::new()
                .with("image_mode", "link")
                .with(*name, *value);
            let output = render(fixture, "svg", options);
            roxmltree::Document::parse(&output).unwrap_or_else(|error| {
                panic!("{stem} with {name}={value}: the SVG should be well-formed XML: {error}")
            });
            insta::assert_snapshot!(format!("svg-{snapshot}-{stem}"), output);
        }
    }
}

#[test]
fn html_overlay_fitting_options() {
    support::prepare();
    for stem in FITTED {
        let fixture = fixture(stem);
        for (snapshot, name, value) in [
            ("char-positions", "var.char_positions", "true"),
            ("cap-height", "var.font_size_mode", "cap_height"),
        ] {
            let options = Options::new()
                .with("template", "html-overlay")
                .with(name, value);
            let output = render_linked(fixture, "template", options);
            insta::assert_snapshot!(format!("template-html-overlay-{snapshot}-{stem}"), output);
        }
    }
}

#[test]
fn the_text_of_an_svg_is_the_text_that_was_read() {
    support::prepare();
    for fixture in FIXTURES {
        let expected = fixture.layout().text();
        // Whatever else is asked for, what the document says stays what the
        // recogniser read, down to the spaces between words and the breaks
        // between lines.
        for (name, value) in [
            ("text_mode", "invisible"),
            ("char_positions", "true"),
            ("space_mode", "tspan"),
            ("font_size_scope", "line"),
        ] {
            let options = Options::new()
                .with("image_mode", "none")
                .with("line_break_mode", "tspan")
                .with(name, value);
            let svg = render(fixture, "svg", options);
            assert_eq!(
                text_of(&svg),
                expected,
                "{}: copying the text out with {name}={value} should give back what was read",
                fixture.stem
            );
        }
    }
}

/// What copying the whole of an SVG's text layer yields: every text node
/// under a `<text>` element, in the order the document holds them.
fn text_of(svg: &str) -> String {
    let document = roxmltree::Document::parse(svg).expect("the SVG is well-formed XML");
    document
        .descendants()
        .filter(|node| node.is_text() && node.ancestors().any(|node| node.has_tag_name("text")))
        .filter_map(|node| node.text())
        .collect()
}

/// The fixture of that name.
fn fixture(stem: &str) -> &'static Fixture {
    FIXTURES
        .iter()
        .find(|fixture| fixture.stem == stem)
        .unwrap_or_else(|| panic!("the set has a `{stem}` fixture"))
}

#[test]
fn two_fixtures_in_one_page_share_no_name() {
    support::prepare();
    // Everything written here is meant to be placed inside somebody else's
    // HTML, where an id is the most collision-prone name there is: two of
    // these in a page must not resolve to one another.
    let svg = Options::new().with("ids", true).with("image_mode", "link");
    let template = |name: &str| Options::new().with("template", name);
    for (renderer, options) in [
        ("svg", svg),
        ("template", template("svg-overlay")),
        ("template", template("html-figure")),
        ("template", template("sr-only-transcript")),
        ("template", template("layout-json")),
    ] {
        let mut seen: Vec<(String, String)> = Vec::new();
        for fixture in FIXTURES {
            for id in ids(&render_linked(fixture, renderer, options.clone())) {
                if let Some((stem, _)) = seen.iter().find(|(_, other)| *other == id) {
                    panic!(
                        "{} and {} both write `{id}` through {renderer}",
                        stem, fixture.stem
                    );
                }
                seen.push((fixture.stem.to_string(), id));
            }
        }
        assert!(
            !seen.is_empty(),
            "{renderer} should write ids, or this promises nothing"
        );
    }
}

/// Every id in a document, however deeply it is nested.
fn ids(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once(" id=\"") {
        let (id, tail) = after.split_once('"').expect("an attribute is closed");
        found.push(id.to_string());
        rest = tail;
    }
    found
}

#[test]
fn template_output() {
    support::prepare();
    for fixture in FIXTURES {
        for template in list_templates() {
            let options = Options::new().with("template", template);
            let output = render(fixture, "template", options);
            insta::assert_snapshot!(format!("template-{template}-{}", fixture.stem), output);
        }
    }
}
