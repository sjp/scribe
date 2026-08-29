//! The layout as a JSON document.
//!
//! This is the layout model itself, written out: the same document
//! [`Layout::to_json`] produces, and one [`Layout::from_json`] reads back.
//! Callers who want the analysis rather than a picture of it — a browser
//! extension, a search index, a script — take this and go.
//!
//! Two options trim or extend it. Dropping the per-character boxes makes for
//! a far smaller document when a consumer works word by word, and adding the
//! source image makes the document self-contained, at the cost of no longer
//! being a bare layout.

use serde_json::Value;

use super::{OptionKind, OptionSpec, OptionValue, Options, RenderError, RenderOutput, Renderer};
use crate::image_source::ImageSource;
use crate::layout::Layout;

/// The name this renderer is registered under.
const NAME: &str = "json";

/// The field the encoded source image is added under.
const IMAGE_DATA_URI: &str = "image_data_uri";

/// Writes the layout as JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonRenderer;

impl Renderer for JsonRenderer {
    fn name(&self) -> &str {
        NAME
    }

    fn describe_options(&self) -> Vec<OptionSpec> {
        vec![
            OptionSpec::new(
                "pretty",
                OptionKind::Bool,
                OptionValue::Bool(true),
                "Indent the document over several lines instead of writing it on one.",
            ),
            OptionSpec::new(
                "include_chars",
                OptionKind::Bool,
                OptionValue::Bool(true),
                "Keep the per-character boxes, which are most of the document's size.",
            ),
            OptionSpec::new(
                "include_image",
                OptionKind::Bool,
                OptionValue::Bool(false),
                "Add the source image as an `image_data_uri` field, if its bytes are known.",
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

        let mut document =
            serde_json::to_value(layout).map_err(|error| RenderError::write(NAME, error))?;
        if !options.bool("include_chars") {
            strip_chars(&mut document);
        }
        if options.bool("include_image")
            && let Some(uri) = image.data_uri()
            && let Some(fields) = document.as_object_mut()
        {
            fields.insert(IMAGE_DATA_URI.to_string(), Value::String(uri));
        }

        let text = if options.bool("pretty") {
            serde_json::to_string_pretty(&document)
        } else {
            serde_json::to_string(&document)
        }
        .map_err(|error| RenderError::write(NAME, error))?;
        Ok(RenderOutput::text(text, "application/json", "json"))
    }
}

/// Drops the `chars` field from every word, leaving no trace of it rather
/// than an empty array.
fn strip_chars(document: &mut Value) {
    let words = document
        .get_mut("lines")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|line| line.get_mut("words"))
        .filter_map(Value::as_array_mut)
        .flatten();
    for word in words {
        if let Some(fields) = word.as_object_mut() {
            fields.shift_remove("chars");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Char, Line, Rect, RotatedBox, Word};

    /// The smallest PNG that decoders accept: one opaque black pixel.
    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a,
        0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn sample_layout() -> Layout {
        Layout::new(
            80,
            24,
            vec![Line {
                text: "hi".to_string(),
                bbox: Rect::new(1.0, 2.0, 20.0, 8.0),
                rotated_box: RotatedBox::new(11.0, 6.0, 20.0, 8.0, 0.0),
                words: vec![Word {
                    text: "hi".to_string(),
                    bbox: Rect::new(1.0, 2.0, 20.0, 8.0),
                    rotated_box: RotatedBox::new(11.0, 6.0, 20.0, 8.0, 0.0),
                    chars: vec![Char {
                        text: "h".to_string(),
                        bbox: Rect::new(1.0, 2.0, 9.0, 8.0),
                        confidence: None,
                    }],
                    confidence: Some(0.9),
                }],
                confidence: Some(0.9),
            }],
        )
    }

    fn render(options: Options) -> String {
        let image = ImageSource::new(80, 24)
            .with_mime("image/png")
            .with_bytes(PIXEL_PNG);
        JsonRenderer
            .render(&sample_layout(), &image, &options)
            .expect("the sample layout renders")
            .as_str()
            .expect("JSON is text")
            .to_string()
    }

    #[test]
    fn the_document_is_the_layout_itself() {
        let output = JsonRenderer
            .render(&sample_layout(), &ImageSource::new(80, 24), &Options::new())
            .unwrap();
        assert_eq!(
            (&*output.mime, &*output.extension),
            ("application/json", "json")
        );
        assert_eq!(
            Layout::from_json(output.as_str().unwrap()).unwrap(),
            sample_layout()
        );
    }

    #[test]
    fn it_is_indented_until_asked_for_one_line() {
        assert!(render(Options::new()).contains('\n'));

        let compact = render(Options::new().with("pretty", false));
        assert!(!compact.contains('\n'), "{compact}");
        assert!(compact.starts_with(r#"{"version":1,"image":{"width":80,"height":24}"#));
    }

    #[test]
    fn fields_keep_the_order_the_model_declares_them_in() {
        let compact = render(Options::new().with("pretty", false));
        let order: Vec<_> = ["version", "image", "lines", "text", "bbox", "rotated_box"]
            .iter()
            .map(|field| compact.find(&format!("\"{field}\"")).expect(field))
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{compact}");
    }

    #[test]
    fn characters_can_be_left_out_altogether() {
        let with_chars = render(Options::new().with("pretty", false));
        assert!(
            with_chars.contains(r#""chars":[{"text":"h""#),
            "{with_chars}"
        );

        let without = render(
            Options::new()
                .with("pretty", false)
                .with("include_chars", false),
        );
        assert!(!without.contains("chars"), "{without}");
        assert!(without.contains(r#""words":[{"text":"hi""#), "{without}");
        assert_eq!(
            Layout::from_json(&without).unwrap().lines[0].words[0].chars,
            Vec::new()
        );
    }

    #[test]
    fn the_image_is_embedded_only_when_asked_for() {
        assert!(!render(Options::new()).contains(IMAGE_DATA_URI));

        let embedded = render(Options::new().with("include_image", true));
        assert!(
            embedded.contains(r#""image_data_uri": "data:image/png;base64,iVBOR"#),
            "{embedded}"
        );
    }

    #[test]
    fn an_image_whose_bytes_are_unknown_is_left_out() {
        let output = JsonRenderer
            .render(
                &sample_layout(),
                &ImageSource::new(80, 24).with_href("scan.png"),
                &Options::new().with("include_image", true),
            )
            .unwrap();
        let text = output.as_str().unwrap();
        assert!(!text.contains(IMAGE_DATA_URI), "{text}");
        assert_eq!(Layout::from_json(text).unwrap(), sample_layout());
    }

    #[test]
    fn an_option_it_does_not_take_is_rejected() {
        let error = JsonRenderer
            .render(
                &Layout::empty(1, 1),
                &ImageSource::new(1, 1),
                &Options::new().with("indent", 2_i64),
            )
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "the json renderer has no `indent` option; it takes `pretty`, `include_chars`, `include_image`"
        );
    }

    #[test]
    fn an_empty_layout_still_renders() {
        let output = JsonRenderer
            .render(
                &Layout::empty(0, 0),
                &ImageSource::new(0, 0),
                &Options::new().with("pretty", false),
            )
            .unwrap();
        assert_eq!(
            output.as_str(),
            Some(r#"{"version":1,"image":{"width":0,"height":0},"lines":[]}"#)
        );
    }
}
