//! What the built-in templates actually write.
//!
//! The layout is hand-built rather than recognised, so that the snapshots
//! stay legible and the same two lines exercise every template: one level,
//! one turned, and text carrying the characters that markup formats have to
//! escape.
//!
//! The image is linked to rather than carried, since a template's handling of
//! an embedded image says no more than four kilobytes of base64 would.

use scribe_core::image_source::ImageSource;
use scribe_core::layout::{Layout, Line, Rect, RotatedBox, Word};
use scribe_core::render::{Options, RenderError, list_templates, registry};

/// The size of the page the layout was read from.
const SIZE: (u32, u32) = (284, 140);

/// A word of the given text, filling the band from `top` to `bottom` between
/// `left` and `right`.
fn word(text: &str, left: f32, right: f32, band: (f32, f32), confidence: f32) -> Word {
    let bbox = Rect::new(left, band.0, right - left, band.1 - band.0);
    Word {
        text: text.to_string(),
        bbox,
        rotated_box: RotatedBox::from_rect(bbox),
        chars: Vec::new(),
        confidence: Some(confidence),
    }
}

/// A line holding the given words, turned by `degrees` about its own centre
/// as a photograph of a tilted page would give.
fn line(words: Vec<Word>, degrees: f32, confidence: Option<f32>) -> Line {
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bbox = Rect::from_points(words.iter().flat_map(|word| {
        [
            (word.bbox.x, word.bbox.y),
            (word.bbox.right(), word.bbox.bottom()),
        ]
    }))
    .expect("a line has words");

    let centre = (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let turn = |box_: RotatedBox| {
        let (x, y) = (box_.cx - centre.0, box_.cy - centre.1);
        RotatedBox::new(
            centre.0 + x * cos - y * sin,
            centre.1 + x * sin + y * cos,
            box_.width,
            box_.height,
            box_.angle_deg + degrees,
        )
    };

    let words = words
        .into_iter()
        .map(|word| Word {
            bbox: turn(word.rotated_box).to_rect(),
            rotated_box: turn(word.rotated_box),
            ..word
        })
        .collect();
    let rotated_box = turn(RotatedBox::from_rect(bbox));
    Line {
        text,
        bbox: rotated_box.to_rect(),
        rotated_box,
        words,
        confidence,
    }
}

/// Two lines of a page: one set level, one tilted, and one word the
/// recogniser is unsure of.
fn sample() -> Layout {
    const UPPER: (f32, f32) = (31.0, 63.0);
    const LOWER: (f32, f32) = (90.0, 110.0);
    Layout::new(
        SIZE.0,
        SIZE.1,
        vec![
            line(
                vec![
                    word("Hello", 28.0, 126.0, UPPER, 0.99),
                    word("World", 143.0, 256.0, UPPER, 0.97),
                ],
                0.0,
                Some(0.98),
            ),
            line(
                vec![
                    word("Tilted", 30.0, 120.0, LOWER, 0.88),
                    word("&", 128.0, 140.0, LOWER, 0.62),
                    word("<set>", 150.0, 250.0, LOWER, 0.91),
                ],
                8.0,
                None,
            ),
        ],
    )
}

/// Renders the sample through one of the built-in templates.
fn render(template: &str, options: Options) -> String {
    let registry = registry();
    let renderer = registry.get("template").expect("template is built in");
    let image = ImageSource::new(SIZE.0, SIZE.1).with_href("page.png");
    let output = renderer
        .render(&sample(), &image, &options.with("template", template))
        .unwrap_or_else(|error| panic!("the {template} template should render: {error}"));
    output.as_str().expect("a template writes text").to_string()
}

/// The document, having checked that it parses as XML.
///
/// A document type declaration is allowed, since hOCR is HTML and carries
/// one.
fn well_formed(text: &str) -> roxmltree::Document<'_> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    roxmltree::Document::parse_with_options(text, options)
        .unwrap_or_else(|error| panic!("the document should be well-formed: {error}\n{text}"))
}

/// Every element carrying the given class, which is how hOCR marks up what a
/// thing is.
fn by_class<'a, 'input>(
    document: &'a roxmltree::Document<'input>,
    class: &str,
) -> Vec<roxmltree::Node<'a, 'input>> {
    document
        .descendants()
        .filter(|node| node.attribute("class") == Some(class))
        .collect()
}

/// The value of one property of an hOCR `title`, such as `bbox` or
/// `x_wconf`.
fn property(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<Vec<f64>> {
    node.attribute("title")?
        .split(';')
        .map(str::trim)
        .find_map(|property| property.strip_prefix(name))?
        .split_whitespace()
        .map(|number| number.parse().ok())
        .collect()
}

#[test]
fn html_overlay() {
    insta::assert_snapshot!(render("html-overlay", Options::new()));
}

#[test]
fn hocr() {
    insta::assert_snapshot!(render("hocr", Options::new()));
}

#[test]
fn alto() {
    insta::assert_snapshot!(render("alto", Options::new()));
}

#[test]
fn markdown() {
    insta::assert_snapshot!(render("markdown", Options::new()));
}

#[test]
fn text() {
    insta::assert_snapshot!(render("text", Options::new()));
}

#[test]
fn a_template_can_take_values_of_its_own() {
    insta::assert_snapshot!(render(
        "hocr",
        Options::new().with("var.title", "A tilted <page>")
    ));
}

#[test]
fn hocr_carries_the_classes_and_boxes_the_format_asks_for() {
    let text = render("hocr", Options::new());
    let document = well_formed(&text);

    let system = document
        .descendants()
        .find(|node| node.attribute("name") == Some("ocr-system"))
        .and_then(|node| node.attribute("content"));
    assert_eq!(system, Some("scribe"));

    let pages = by_class(&document, "ocr_page");
    assert_eq!(pages.len(), 1);
    assert_eq!(
        property(&pages[0], "bbox"),
        Some(vec![0.0, 0.0, f64::from(SIZE.0), f64::from(SIZE.1)])
    );
    assert!(
        pages[0]
            .attribute("title")
            .unwrap()
            .contains(r#"image "page.png""#),
        "the page should name the image it was read from"
    );

    let lines = by_class(&document, "ocr_line");
    assert_eq!(lines.len(), sample().lines.len());
    let words = by_class(&document, "ocrx_word");
    assert_eq!(
        words.len(),
        sample()
            .lines
            .iter()
            .map(|line| line.words.len())
            .sum::<usize>()
    );

    for item in lines.iter().chain(words.iter()) {
        let bbox = property(item, "bbox").expect("every item carries a bbox");
        assert_eq!(bbox.len(), 4, "a bbox is two corners");
        assert!(bbox[0] <= bbox[2] && bbox[1] <= bbox[3], "{bbox:?}");
        assert!(
            bbox.iter().all(|edge| edge.fract() == 0.0),
            "hOCR counts in whole pixels: {bbox:?}"
        );
    }

    // The recogniser was unsure of one word and said nothing about one line,
    // so the confidences are there where they are known and nowhere else.
    let confidences: Vec<_> = words.iter().map(|word| property(word, "x_wconf")).collect();
    assert_eq!(
        confidences,
        [
            Some(vec![99.0]),
            Some(vec![97.0]),
            Some(vec![88.0]),
            Some(vec![62.0]),
            Some(vec![91.0]),
        ]
    );
    assert_eq!(
        lines
            .iter()
            .map(|line| property(line, "x_wconf"))
            .collect::<Vec<_>>(),
        [None, None]
    );
}

#[test]
fn the_markup_templates_are_well_formed() {
    for template in ["html-overlay", "hocr", "alto"] {
        let text = render(template, Options::new());
        let document = well_formed(&text);
        assert!(
            text.contains("&lt;set&gt;"),
            "{template} should escape the text it writes:\n{text}"
        );
        assert!(
            document.descendants().count() > 1,
            "{template} should write more than a root element"
        );
    }
}

#[test]
fn alto_places_every_word_it_recognised() {
    let text = render("alto", Options::new());
    let document = well_formed(&text);
    let strings: Vec<_> = document
        .descendants()
        .filter(|node| node.has_tag_name("String"))
        .filter_map(|node| node.attribute("CONTENT"))
        .collect();
    assert_eq!(strings, ["Hello", "World", "Tilted", "&", "<set>"]);
}

#[test]
fn plain_text_is_the_text_and_nothing_more() {
    assert_eq!(
        render("text", Options::new()),
        "Hello World\nTilted & <set>\n"
    );
}

#[test]
fn a_template_of_ones_own_needs_no_built_in() {
    let registry = registry();
    let renderer = registry.get("template").expect("template is built in");
    let output = renderer
        .render(
            &sample(),
            &ImageSource::new(SIZE.0, SIZE.1),
            &Options::new().with("template_source", "{{ text }}"),
        )
        .expect("a template of one's own renders");
    assert_eq!(output.as_str(), Some("Hello World\nTilted & <set>"));
}

#[test]
fn a_name_that_is_not_a_template_says_which_ones_are() {
    let registry = registry();
    let renderer = registry.get("template").expect("template is built in");
    let error = renderer
        .render(
            &sample(),
            &ImageSource::new(SIZE.0, SIZE.1),
            &Options::new().with("template", "yaml"),
        )
        .expect_err("there is no yaml template");
    assert!(
        matches!(error, RenderError::InvalidChoice { .. }),
        "{error:?}"
    );
    let message = error.to_string();
    for name in list_templates() {
        assert!(message.contains(&format!("`{name}`")), "{message}");
    }
}

#[test]
fn every_template_that_is_offered_renders() {
    for template in list_templates() {
        assert!(
            !render(template, Options::new()).is_empty(),
            "{template} should write something"
        );
    }
}

#[test]
fn the_registry_offers_every_built_in_renderer() {
    let registry = registry();
    assert_eq!(registry.names(), ["json", "svg", "template"]);
    for name in registry.names() {
        let renderer = registry.get(name).expect("the name came from the registry");
        assert!(
            !renderer.describe_options().is_empty(),
            "{name} should describe its options"
        );
    }
}
