//! The renderer-agnostic description of the text found in an image.
//!
//! A layout is the contract between the OCR pipeline and every renderer, and
//! between this crate and its callers: it is versioned, serialisable, and the
//! only thing a renderer needs in order to do its job. Text is described at
//! three granularities — line, word and character — and each item carries both
//! an axis-aligned bounding box and an oriented box, in image pixel
//! coordinates with the origin at the top left and y increasing downwards.
//!
//! # JSON
//!
//! [`Layout`] round-trips through JSON with `snake_case` field names. Unknown
//! fields are rejected, so a document written against a different shape of the
//! model fails loudly instead of losing data silently. Absent collections and
//! confidences are the only omissions accepted on input; output always spells
//! them out.
//!
//! ```
//! use scribe_core::layout::Layout;
//!
//! let layout = Layout::empty(800, 600);
//! let json = layout.to_json().unwrap();
//! assert_eq!(Layout::from_json(&json).unwrap(), layout);
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The schema version written to every [`Layout`] this build produces.
///
/// It changes only when the meaning or the shape of the model changes, so a
/// consumer can decide whether it understands a document before reading it.
pub const LAYOUT_VERSION: u32 = 1;

/// Everything scribe knows about the text in one image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// The version of the model this document was written against.
    ///
    /// The current one is `LAYOUT_VERSION`.
    pub version: u32,
    /// The raster the coordinates in this layout refer to.
    pub image: ImageInfo,
    /// The lines of text, in reading order.
    #[serde(default)]
    pub lines: Vec<Line>,
}

impl Layout {
    /// Builds a layout of the given lines over an image of the given size,
    /// stamped with [`LAYOUT_VERSION`].
    pub fn new(width: u32, height: u32, lines: Vec<Line>) -> Self {
        Self {
            version: LAYOUT_VERSION,
            image: ImageInfo::new(width, height),
            lines,
        }
    }

    /// Builds a layout with no text over an image of the given size.
    ///
    /// This is what an image with no recognisable text produces, and it is
    /// still enough for a renderer to reproduce the image itself.
    pub fn empty(width: u32, height: u32) -> Self {
        Self::new(width, height, Vec::new())
    }

    /// The whole text of the image, one line per line, joined with `\n`.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The JSON Schema describing this model.
    ///
    /// Consumers in other languages can generate their own types from it, and
    /// documents can be validated against it before being read.
    pub fn json_schema() -> schemars::Schema {
        schemars::schema_for!(Layout)
    }

    /// Reads a layout from its JSON representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the document is not valid JSON or does not match
    /// the model, including when it carries fields the model does not know.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Writes the layout as compact JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if any coordinate or confidence is not a finite
    /// number, since JSON cannot represent those.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Writes the layout as indented JSON.
    ///
    /// # Errors
    ///
    /// As [`Layout::to_json`].
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// The raster a layout describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageInfo {
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
}

impl ImageInfo {
    /// Describes an image of the given pixel dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// A line of recognised text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Line {
    /// The text of the line, ready to be used as-is by a renderer.
    pub text: String,
    /// The axis-aligned bounds of the line.
    pub bbox: Rect,
    /// The oriented bounds of the line.
    pub rotated_box: RotatedBox,
    /// The words of the line, in reading order.
    #[serde(default)]
    pub words: Vec<Word>,
    /// How sure the recogniser is of this line, from 0 to 1, when it says.
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// A word within a line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Word {
    /// The text of the word.
    pub text: String,
    /// The axis-aligned bounds of the word.
    pub bbox: Rect,
    /// The oriented bounds of the word.
    pub rotated_box: RotatedBox,
    /// The characters of the word, in reading order.
    ///
    /// Renderers position words by default; this is here for the ones that
    /// want finer control, and may be empty when the recogniser offers
    /// nothing per character.
    #[serde(default)]
    pub chars: Vec<Char>,
    /// How sure the recogniser is of this word, from 0 to 1, when it says.
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// A single character within a word.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Char {
    /// The character itself, as one grapheme cluster.
    pub text: String,
    /// The axis-aligned bounds of the character.
    pub bbox: Rect,
    /// How sure the recogniser is of this character, from 0 to 1, when it
    /// says.
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// An axis-aligned rectangle in image pixels, with the origin at the top left
/// and y increasing downwards.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    /// The x coordinate of the left edge.
    pub x: f32,
    /// The y coordinate of the top edge.
    pub y: f32,
    /// The width, extending to the right.
    pub width: f32,
    /// The height, extending downwards.
    pub height: f32,
}

impl Rect {
    /// A rectangle with the given top-left corner and size.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The smallest rectangle containing every one of the given points, or
    /// `None` if there are no points.
    pub fn from_points<I>(points: I) -> Option<Self>
    where
        I: IntoIterator<Item = (f32, f32)>,
    {
        let mut points = points.into_iter();
        let (first_x, first_y) = points.next()?;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (first_x, first_y, first_x, first_y);
        for (x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        Some(Self::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// The x coordinate of the right edge.
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The y coordinate of the bottom edge.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// An oriented rectangle in image pixels: a box of `width` by `height` centred
/// on `(cx, cy)` and turned by `angle_deg`.
///
/// The angle is measured from the positive x axis to the box's width axis and
/// is positive clockwise as seen on screen, which is what SVG's `rotate()`
/// does in the same y-down coordinate system.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotatedBox {
    /// The x coordinate of the centre.
    pub cx: f32,
    /// The y coordinate of the centre.
    pub cy: f32,
    /// The extent along the box's width axis.
    pub width: f32,
    /// The extent along the box's height axis.
    pub height: f32,
    /// The rotation in degrees, clockwise-positive, normalised to
    /// `(-180, 180]`.
    pub angle_deg: f32,
}

impl RotatedBox {
    /// A box with the given centre, size and rotation, the angle normalised
    /// to `(-180, 180]`.
    pub fn new(cx: f32, cy: f32, width: f32, height: f32, angle_deg: f32) -> Self {
        Self {
            cx,
            cy,
            width,
            height,
            angle_deg: normalize_angle_deg(angle_deg),
        }
    }

    /// The unrotated box covering the given rectangle.
    pub fn from_rect(rect: Rect) -> Self {
        Self::new(
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
            rect.width,
            rect.height,
            0.0,
        )
    }

    /// The four corners, starting at the corner that is the top left before
    /// rotation and going clockwise on screen.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (sin, cos) = self.angle_deg.to_radians().sin_cos();
        let (half_width, half_height) = (self.width / 2.0, self.height / 2.0);
        [
            (-half_width, -half_height),
            (half_width, -half_height),
            (half_width, half_height),
            (-half_width, half_height),
        ]
        .map(|(x, y)| (self.cx + x * cos - y * sin, self.cy + x * sin + y * cos))
    }

    /// The smallest axis-aligned rectangle containing the box.
    pub fn to_rect(&self) -> Rect {
        Rect::from_points(self.corners()).expect("a box always has four corners")
    }

    /// Whether the box's sides are parallel to the image axes, to within
    /// `tolerance_deg`.
    ///
    /// This holds at every multiple of 90 degrees, so a box that is
    /// axis-aligned may still be a quarter turn from upright, in which case
    /// [`RotatedBox::to_rect`] swaps its width and height.
    pub fn is_axis_aligned(&self, tolerance_deg: f32) -> bool {
        let past_quarter_turn = self.angle_deg.rem_euclid(90.0);
        past_quarter_turn.min(90.0 - past_quarter_turn) <= tolerance_deg.abs()
    }
}

/// Wraps an angle in degrees into `(-180, 180]`.
///
/// Non-finite angles are returned unchanged, since there is no equivalent for
/// them in the range.
pub fn normalize_angle_deg(degrees: f32) -> f32 {
    if !degrees.is_finite() {
        return degrees;
    }
    let wrapped = degrees.rem_euclid(360.0);
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half a pixel of a thousandth: tight enough to catch a wrong sign or a
    /// swapped axis, loose enough for `f32` trigonometry.
    const EPSILON: f32 = 1e-4;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_corners_close(actual: [(f32, f32); 4], expected: [(f32, f32); 4]) {
        for (index, ((x, y), (want_x, want_y))) in actual.iter().zip(expected.iter()).enumerate() {
            assert!(
                (x - want_x).abs() <= EPSILON && (y - want_y).abs() <= EPSILON,
                "corner {index}: expected {expected:?}, got {actual:?}"
            );
        }
    }

    fn sample_layout() -> Layout {
        Layout::new(
            640,
            480,
            vec![Line {
                text: "hello world".to_string(),
                bbox: Rect::new(10.0, 20.0, 100.0, 18.0),
                rotated_box: RotatedBox::new(60.0, 29.0, 100.0, 18.0, 2.5),
                words: vec![
                    Word {
                        text: "hello".to_string(),
                        bbox: Rect::new(10.0, 20.0, 44.0, 18.0),
                        rotated_box: RotatedBox::new(32.0, 29.0, 44.0, 18.0, 2.5),
                        chars: vec![Char {
                            text: "h".to_string(),
                            bbox: Rect::new(10.0, 20.0, 9.0, 18.0),
                            confidence: Some(0.97),
                        }],
                        confidence: Some(0.91),
                    },
                    Word {
                        text: "world".to_string(),
                        bbox: Rect::new(62.0, 20.0, 48.0, 18.0),
                        rotated_box: RotatedBox::new(86.0, 29.0, 48.0, 18.0, 2.5),
                        chars: Vec::new(),
                        confidence: None,
                    },
                ],
                confidence: Some(0.88),
            }],
        )
    }

    #[test]
    fn json_round_trips() {
        let layout = sample_layout();
        assert_eq!(
            Layout::from_json(&layout.to_json().unwrap()).unwrap(),
            layout
        );
        assert_eq!(
            Layout::from_json(&layout.to_json_pretty().unwrap()).unwrap(),
            layout
        );
    }

    #[test]
    fn json_uses_snake_case_and_rejects_unknown_fields() {
        let json = sample_layout().to_json().unwrap();
        assert!(json.contains("\"rotated_box\""));
        assert!(json.contains("\"angle_deg\""));

        let stray = json.replacen("{\"version\":1,", "{\"version\":1,\"colour\":\"red\",", 1);
        assert!(Layout::from_json(&stray).is_err());
    }

    #[test]
    fn optional_parts_may_be_omitted_on_input() {
        let json = r#"{"version":1,"image":{"width":8,"height":4}}"#;
        assert_eq!(Layout::from_json(json).unwrap(), Layout::empty(8, 4));
    }

    #[test]
    fn empty_layout_is_versioned_and_textless() {
        let layout = Layout::empty(1920, 1080);
        assert_eq!(layout.version, LAYOUT_VERSION);
        assert_eq!(layout.image, ImageInfo::new(1920, 1080));
        assert!(layout.lines.is_empty());
        assert_eq!(layout.text(), "");
    }

    #[test]
    fn text_joins_lines_with_newlines() {
        let line = |text: &str| Line {
            text: text.to_string(),
            bbox: Rect::new(0.0, 0.0, 1.0, 1.0),
            rotated_box: RotatedBox::new(0.5, 0.5, 1.0, 1.0, 0.0),
            words: Vec::new(),
            confidence: None,
        };
        let layout = Layout::new(10, 10, vec![line("one"), line("two"), line("three")]);
        assert_eq!(layout.text(), "one\ntwo\nthree");
    }

    #[test]
    fn rect_from_points_bounds_every_point() {
        let rect = Rect::from_points([(3.0, -1.0), (-2.0, 4.0), (1.0, 1.0)]).unwrap();
        assert_eq!(rect, Rect::new(-2.0, -1.0, 5.0, 5.0));
        assert_eq!(rect.right(), 3.0);
        assert_eq!(rect.bottom(), 4.0);

        let single = Rect::from_points([(7.0, 9.0)]).unwrap();
        assert_eq!(single, Rect::new(7.0, 9.0, 0.0, 0.0));
        assert_eq!(Rect::from_points([]), None);
    }

    #[test]
    fn corners_of_an_unrotated_box_start_at_the_top_left() {
        let corners = RotatedBox::new(10.0, 20.0, 4.0, 2.0, 0.0).corners();
        assert_corners_close(
            corners,
            [(8.0, 19.0), (12.0, 19.0), (12.0, 21.0), (8.0, 21.0)],
        );
    }

    #[test]
    fn corners_turn_clockwise_on_screen() {
        // A quarter turn clockwise sends the width axis straight down, so the
        // corner that was at the top left ends up at the top right.
        let corners = RotatedBox::new(0.0, 0.0, 4.0, 2.0, 90.0).corners();
        assert_corners_close(
            corners,
            [(1.0, -2.0), (1.0, 2.0), (-1.0, 2.0), (-1.0, -2.0)],
        );
    }

    #[test]
    fn corners_of_a_negatively_rotated_box_turn_anticlockwise() {
        let corners = RotatedBox::new(0.0, 0.0, 4.0, 2.0, -90.0).corners();
        assert_corners_close(
            corners,
            [(-1.0, 2.0), (-1.0, -2.0), (1.0, -2.0), (1.0, 2.0)],
        );
    }

    #[test]
    fn corners_at_forty_five_degrees_sit_on_the_diagonals() {
        let half_diagonal = 2.0_f32.sqrt();
        let corners = RotatedBox::new(0.0, 0.0, 2.0, 2.0, 45.0).corners();
        assert_corners_close(
            corners,
            [
                (0.0, -half_diagonal),
                (half_diagonal, 0.0),
                (0.0, half_diagonal),
                (-half_diagonal, 0.0),
            ],
        );

        let corners = RotatedBox::new(0.0, 0.0, 2.0, 2.0, -45.0).corners();
        assert_corners_close(
            corners,
            [
                (-half_diagonal, 0.0),
                (0.0, -half_diagonal),
                (half_diagonal, 0.0),
                (0.0, half_diagonal),
            ],
        );
    }

    #[test]
    fn to_rect_bounds_the_rotation() {
        let rect = RotatedBox::new(5.0, 5.0, 4.0, 2.0, 0.0).to_rect();
        assert_eq!(rect, Rect::new(3.0, 4.0, 4.0, 2.0));

        let turned = RotatedBox::new(5.0, 5.0, 4.0, 2.0, 90.0).to_rect();
        assert_close(turned.x, 4.0);
        assert_close(turned.y, 3.0);
        assert_close(turned.width, 2.0);
        assert_close(turned.height, 4.0);

        let diagonal = RotatedBox::new(0.0, 0.0, 2.0, 2.0, 45.0).to_rect();
        assert_close(diagonal.width, 2.0 * 2.0_f32.sqrt());
        assert_close(diagonal.height, 2.0 * 2.0_f32.sqrt());
    }

    #[test]
    fn from_rect_covers_the_rectangle_without_turning_it() {
        let rect = Rect::new(2.0, 4.0, 10.0, 6.0);
        let box_ = RotatedBox::from_rect(rect);
        assert_eq!(box_, RotatedBox::new(7.0, 7.0, 10.0, 6.0, 0.0));
        assert_eq!(box_.to_rect(), rect);
    }

    #[test]
    fn axis_alignment_holds_at_every_quarter_turn() {
        for angle in [0.0, 90.0, -90.0, 180.0, 0.4, -89.6] {
            assert!(
                RotatedBox::new(0.0, 0.0, 2.0, 1.0, angle).is_axis_aligned(0.5),
                "{angle} degrees should count as axis aligned"
            );
        }
        for angle in [1.0, 45.0, -45.0, 89.0, 179.0] {
            assert!(
                !RotatedBox::new(0.0, 0.0, 2.0, 1.0, angle).is_axis_aligned(0.5),
                "{angle} degrees should not count as axis aligned"
            );
        }
    }

    #[test]
    fn angles_normalise_into_a_half_turn_either_side() {
        assert_eq!(normalize_angle_deg(0.0), 0.0);
        assert_eq!(normalize_angle_deg(180.0), 180.0);
        assert_eq!(normalize_angle_deg(-180.0), 180.0);
        assert_eq!(normalize_angle_deg(181.0), -179.0);
        assert_eq!(normalize_angle_deg(-190.0), 170.0);
        assert_eq!(normalize_angle_deg(360.0), 0.0);
        assert_eq!(normalize_angle_deg(725.0), 5.0);
        assert_eq!(normalize_angle_deg(-725.0), -5.0);
        assert!(normalize_angle_deg(f32::NAN).is_nan());

        assert_eq!(RotatedBox::new(0.0, 0.0, 1.0, 1.0, 450.0).angle_deg, 90.0);
    }
}
