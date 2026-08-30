//! Reads a real image with the real models.
//!
//! The models are large and licensed separately, so they are not in this
//! repository. Point `SCRIBE_DETECTION_MODEL` and `SCRIBE_RECOGNITION_MODEL`
//! at a copy of them — `scripts/fetch-models.sh` downloads one — to run these
//! tests; without them there is nothing to run against and they pass with a
//! notice instead.

use std::path::{Path, PathBuf};

use scribe_core::layout::Layout;
use scribe_core::ocr::{Channels, Engine, Models, OcrOptions, PixelImage};

/// Set this to the path of the text detection model.
const DETECTION_VARIABLE: &str = "SCRIBE_DETECTION_MODEL";

/// Set this to the path of the text recognition model.
const RECOGNITION_VARIABLE: &str = "SCRIBE_RECOGNITION_MODEL";

/// The models, or `None` with a printed notice if the environment does not
/// say where they are.
fn models() -> Option<Models> {
    let read = |variable: &str| {
        let path = PathBuf::from(std::env::var_os(variable)?);
        Some(std::fs::read(&path).unwrap_or_else(|error| {
            panic!("{variable} points at {}, which {error}", path.display())
        }))
    };
    match (read(DETECTION_VARIABLE), read(RECOGNITION_VARIABLE)) {
        (Some(detection), Some(recognition)) => Some(Models::new(detection, recognition)),
        _ => {
            println!(
                "skipped: set {DETECTION_VARIABLE} and {RECOGNITION_VARIABLE} to run this test"
            );
            None
        }
    }
}

/// The rendered "Hello World" fixture, as pixels.
fn hello_world() -> (u32, u32, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/hello.png");
    let image = image::open(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
        .into_rgb8();
    let (width, height) = image.dimensions();
    (width, height, image.into_raw())
}

/// Analyses the fixture with the given options, or `None` if the models are
/// not available.
fn analyze_hello_world(options: OcrOptions) -> Option<Layout> {
    let models = models()?;
    let engine = Engine::new(models, options).expect("the models load");
    let (width, height, pixels) = hello_world();
    let image = PixelImage::new(width, height, Channels::Rgb, &pixels);
    Some(engine.analyze(&image).expect("the fixture is analysed"))
}

#[test]
fn the_words_of_a_rendered_image_are_read_and_placed() {
    let Some(layout) = analyze_hello_world(OcrOptions::default()) else {
        return;
    };

    let (width, height) = (layout.image.width, layout.image.height);
    assert!(!layout.lines.is_empty(), "the fixture has a line of text");
    assert_eq!(
        layout.text().to_lowercase().replace(' ', ""),
        "helloworld",
        "read {:?}",
        layout.text()
    );

    for line in &layout.lines {
        // The text is rendered upright, so the engine should see it that way.
        assert!(
            line.rotated_box.is_axis_aligned(5.0),
            "{:?} should be roughly upright, not at {} degrees",
            line.text,
            line.rotated_box.angle_deg
        );
        assert_eq!(line.words.len(), 2, "read {:?}", line.text);

        for word in &line.words {
            assert!(
                word.bbox.x >= 0.0
                    && word.bbox.y >= 0.0
                    && word.bbox.right() <= width as f32
                    && word.bbox.bottom() <= height as f32,
                "{:?} at {:?} is outside a {width} by {height} image",
                word.text,
                word.bbox
            );
            assert_eq!(
                word.chars.len(),
                word.text.chars().count(),
                "{:?} should have a box per character",
                word.text
            );
        }
    }
}

#[test]
fn characters_can_be_left_out() {
    let options = OcrOptions {
        include_chars: false,
        ..OcrOptions::default()
    };
    let Some(layout) = analyze_hello_world(options) else {
        return;
    };

    assert!(!layout.lines.is_empty(), "the fixture has a line of text");
    for line in &layout.lines {
        assert!(!line.words.is_empty(), "{:?} should have words", line.text);
        for word in &line.words {
            assert!(word.chars.is_empty(), "{:?} kept its characters", word.text);
        }
    }
}
