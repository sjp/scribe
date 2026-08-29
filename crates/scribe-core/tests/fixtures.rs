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

/// Renders a fixture's layout the way a caller with the image to hand would,
/// leaving every option at its default.
fn render(fixture: &Fixture, name: &str, options: Options) -> String {
    let registry = registry();
    let renderer = registry
        .get(name)
        .unwrap_or_else(|| panic!("`{name}` is built in"));
    let layout = fixture.layout();
    let bytes = fixture.image_bytes();
    let file_name = fixture.file_name();
    let image = ImageSource::new(layout.image.width, layout.image.height)
        .with_mime(fixture.kind.mime())
        .with_bytes(&bytes)
        .with_href(&file_name);
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
