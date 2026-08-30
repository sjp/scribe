//! Recognition of text in a raster image, producing a layout.
//!
//! This module wraps the OCR engine behind an interface that suits every
//! target the crate supports: models arrive as bytes rather than paths, and
//! images arrive as an RGB(A) pixel buffer rather than a file. Loading those
//! bytes is the caller's problem, which keeps the filesystem out of the
//! library.
//!
//! ```no_run
//! use scribe_core::ocr::{Channels, Engine, Models, OcrOptions, PixelImage};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let (detection, recognition, pixels) = (Vec::new(), Vec::new(), Vec::new());
//! let models = Models::new(detection, recognition);
//! let engine = Engine::new(models, OcrOptions::default())?;
//! let layout = engine.analyze(&PixelImage::new(640, 480, Channels::Rgb, &pixels))?;
//! println!("{}", layout.text());
//! # Ok(())
//! # }
//! ```

use std::fmt;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem, TextLine, TextWord};
use rten_imageproc::{Rect as PixelRect, RotatedRect};
use thiserror::Error;

use crate::layout::{Char, Layout, Line, Rect, RotatedBox, Word};

/// The cause of a failure inside the OCR engine, whose own error types are
/// not part of this crate's interface.
pub type EngineError = Box<dyn std::error::Error + Send + Sync>;

/// The trained models the engine runs, as the bytes of two `.rten` files.
///
/// Models are never fetched by this crate; the caller reads them from
/// wherever it keeps them and hands over the bytes.
#[derive(Clone, Debug, Default)]
pub struct Models {
    /// The model that finds where the words are.
    pub detection: Vec<u8>,
    /// The model that reads the words it is shown.
    pub recognition: Vec<u8>,
}

impl Models {
    /// Pairs the bytes of a detection model with those of a recognition
    /// model.
    pub fn new(detection: Vec<u8>, recognition: Vec<u8>) -> Self {
        Self {
            detection,
            recognition,
        }
    }
}

/// Which of the two models a failure concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    /// The model that finds where the words are.
    Detection,
    /// The model that reads the words it is shown.
    Recognition,
}

impl fmt::Display for ModelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Detection => "detection",
            Self::Recognition => "recognition",
        })
    }
}

/// The step of the analysis a failure happened in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Turning the caller's pixels into the engine's input.
    Preparation,
    /// Finding the words in that input.
    Detection,
    /// Reading the words that were found.
    Recognition,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Preparation => "preparing the image",
            Self::Detection => "detecting words",
            Self::Recognition => "recognising text",
        })
    }
}

/// Everything that can go wrong between pixels and a layout.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OcrError {
    /// The bytes given for a model are not one this build can run.
    #[error("the {model} model could not be loaded")]
    ModelLoad {
        /// The model whose bytes were rejected.
        model: ModelKind,
        /// What the model loader said.
        #[source]
        source: EngineError,
    },

    /// The models loaded but the engine would not start on them.
    #[error("the engine could not be started")]
    Start {
        /// What the engine said.
        #[source]
        source: EngineError,
    },

    /// The image has no pixels, so there is nothing to recognise.
    #[error("an image of {width} by {height} pixels has no pixels to read")]
    EmptyImage {
        /// The width that was given.
        width: u32,
        /// The height that was given.
        height: u32,
    },

    /// The pixel buffer is not the size the dimensions and channels imply.
    #[error(
        "{width} by {height} pixels in {channels} need {expected} bytes, but {actual} were given"
    )]
    PixelCount {
        /// The width that was given.
        width: u32,
        /// The height that was given.
        height: u32,
        /// The channel layout that was given.
        channels: Channels,
        /// How many bytes that adds up to.
        expected: usize,
        /// How many bytes the buffer actually holds.
        actual: usize,
    },

    /// The engine failed part way through the analysis.
    #[error("{stage} failed")]
    Analysis {
        /// Where it got to.
        stage: Stage,
        /// What the engine said.
        #[source]
        source: EngineError,
    },

    /// The encoded image could not be decoded into pixels.
    #[cfg(feature = "decode")]
    #[error("the image could not be decoded")]
    Decode {
        /// What the decoder said.
        #[source]
        source: image::ImageError,
    },
}

/// How the recogniser turns the model's output into characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecodeMethod {
    /// Take the likeliest character at every step. Fast, and what most
    /// callers want.
    #[default]
    Greedy,
    /// Keep several candidate readings alive and choose the likeliest whole
    /// sequence. Slower, and sometimes more accurate.
    BeamSearch {
        /// How many candidates to keep.
        width: u32,
    },
}

impl From<DecodeMethod> for ocrs::DecodeMethod {
    fn from(method: DecodeMethod) -> Self {
        match method {
            DecodeMethod::Greedy => Self::Greedy,
            DecodeMethod::BeamSearch { width } => Self::BeamSearch { width },
        }
    }
}

/// How to run the engine.
#[derive(Clone, Debug)]
pub struct OcrOptions {
    /// The characters the recognition model was trained on, in the order it
    /// was trained on them.
    ///
    /// Leave this unset unless you are using a custom recognition model; the
    /// engine then uses the alphabet its own models were trained with.
    pub alphabet: Option<String>,
    /// The only characters recognition may produce, if it should be
    /// restricted to some of the alphabet — digits alone, say.
    pub allowed_chars: Option<String>,
    /// How to turn the model's output into characters.
    pub decode_method: DecodeMethod,
    /// Whether to keep the per-character boxes in the layout.
    ///
    /// They are what a renderer needs to place text character by character,
    /// and they are also the bulk of a serialised layout, so dropping them
    /// makes for a much smaller document.
    pub include_chars: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            alphabet: None,
            allowed_chars: None,
            decode_method: DecodeMethod::default(),
            include_chars: true,
        }
    }
}

/// The number and order of colour channels in a pixel buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    /// One byte of brightness per pixel.
    Gray,
    /// Three bytes per pixel: red, green, blue.
    Rgb,
    /// Four bytes per pixel: red, green, blue, alpha.
    Rgba,
}

impl Channels {
    /// How many bytes each pixel takes up.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

impl fmt::Display for Channels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Gray => "greyscale",
            Self::Rgb => "RGB",
            Self::Rgba => "RGBA",
        })
    }
}

/// A borrowed buffer of pixels, row by row from the top, each row left to
/// right, with the channels of a pixel adjacent.
#[derive(Clone, Copy, Debug)]
pub struct PixelImage<'a> {
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// How the bytes of each pixel are laid out.
    pub channels: Channels,
    /// The pixels, `width * height * channels` bytes of them.
    pub data: &'a [u8],
}

impl<'a> PixelImage<'a> {
    /// Describes a buffer of pixels of the given size and channel layout.
    ///
    /// Nothing is checked here; a buffer whose length does not match is
    /// rejected by [`Engine::analyze`].
    pub fn new(width: u32, height: u32, channels: Channels, data: &'a [u8]) -> Self {
        Self {
            width,
            height,
            channels,
            data,
        }
    }

    /// How many bytes a buffer of this size and channel layout holds.
    fn expected_len(&self) -> usize {
        self.width as usize * self.height as usize * self.channels.bytes_per_pixel()
    }

    /// Hands the pixels to the engine, once they are known to describe the
    /// image they claim to.
    fn to_source(self) -> Result<ImageSource<'a>, OcrError> {
        if self.width == 0 || self.height == 0 {
            return Err(OcrError::EmptyImage {
                width: self.width,
                height: self.height,
            });
        }
        let expected = self.expected_len();
        if self.data.len() != expected {
            return Err(OcrError::PixelCount {
                width: self.width,
                height: self.height,
                channels: self.channels,
                expected,
                actual: self.data.len(),
            });
        }
        ImageSource::from_bytes(self.data, (self.width, self.height)).map_err(|source| {
            OcrError::Analysis {
                stage: Stage::Preparation,
                source: Box::new(source),
            }
        })
    }
}

#[cfg(feature = "decode")]
impl PixelImage<'_> {
    /// Decodes an encoded image — PNG, JPEG, WebP and the rest — into
    /// pixels, keeping as much of the original as the engine can use.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not an image in a format this build
    /// understands.
    pub fn decode(bytes: &[u8]) -> Result<OwnedPixelImage, OcrError> {
        use image::ColorType;

        let decoded =
            image::load_from_memory(bytes).map_err(|source| OcrError::Decode { source })?;
        let (width, height) = (decoded.width(), decoded.height());
        let (channels, data) = match decoded.color() {
            ColorType::L8 | ColorType::L16 => (Channels::Gray, decoded.into_luma8().into_raw()),
            colour if colour.has_alpha() => (Channels::Rgba, decoded.into_rgba8().into_raw()),
            _ => (Channels::Rgb, decoded.into_rgb8().into_raw()),
        };
        Ok(OwnedPixelImage {
            width,
            height,
            channels,
            data,
        })
    }
}

/// A buffer of pixels this crate owns, as decoded from an encoded image.
#[cfg(feature = "decode")]
#[derive(Clone, Debug)]
pub struct OwnedPixelImage {
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// How the bytes of each pixel are laid out.
    pub channels: Channels,
    /// The pixels, `width * height * channels` bytes of them.
    pub data: Vec<u8>,
}

#[cfg(feature = "decode")]
impl OwnedPixelImage {
    /// Borrows the pixels for [`Engine::analyze`].
    pub fn as_pixel_image(&self) -> PixelImage<'_> {
        PixelImage::new(self.width, self.height, self.channels, &self.data)
    }
}

/// A loaded pair of models, ready to read images.
///
/// Building one is expensive and reading an image is not, so build the engine
/// once and analyse as many images with it as you like.
///
/// An engine owns the bytes of both models for as long as it lives: the
/// weights are read in place out of those buffers rather than copied into
/// structures of their own. Tens of megabytes stay resident per engine, so
/// drop one when there is nothing left to read.
pub struct Engine {
    engine: OcrEngine,
    include_chars: bool,
}

impl Engine {
    /// Loads the models and starts an engine that runs them with the given
    /// options.
    ///
    /// The bytes are taken, not copied: the engine keeps them for its
    /// lifetime.
    ///
    /// # Errors
    ///
    /// Returns an error if either model cannot be loaded or the engine
    /// rejects them.
    pub fn new(models: Models, options: OcrOptions) -> Result<Self, OcrError> {
        let detection = load_model(ModelKind::Detection, models.detection)?;
        let recognition = load_model(ModelKind::Recognition, models.recognition)?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection),
            recognition_model: Some(recognition),
            alphabet: options.alphabet,
            allowed_chars: options.allowed_chars,
            decode_method: options.decode_method.into(),
            ..Default::default()
        })
        .map_err(|error| OcrError::Start {
            source: error.into(),
        })?;
        Ok(Self {
            engine,
            include_chars: options.include_chars,
        })
    }

    /// Reads the text in an image.
    ///
    /// Lines the recogniser could make nothing of are left out; an image with
    /// no text at all gives a layout with no lines, which is still enough for
    /// a renderer to reproduce the image.
    ///
    /// No line, word or character comes back with a confidence: the engine
    /// reports no score for what it read.
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer does not describe an image of the size
    /// it claims, or if the engine fails.
    pub fn analyze(&self, image: &PixelImage<'_>) -> Result<Layout, OcrError> {
        let input = self
            .engine
            .prepare_input(image.to_source()?)
            .map_err(|error| OcrError::Analysis {
                stage: Stage::Preparation,
                source: error.into(),
            })?;
        let words = self
            .engine
            .detect_words(&input)
            .map_err(|error| OcrError::Analysis {
                stage: Stage::Detection,
                source: error.into(),
            })?;
        let line_rects = self.engine.find_text_lines(&input, &words);
        let recognised = self
            .engine
            .recognize_text(&input, &line_rects)
            .map_err(|error| OcrError::Analysis {
                stage: Stage::Recognition,
                source: error.into(),
            })?;

        let lines = recognised
            .iter()
            .flatten()
            .map(|line| convert_line(line, self.include_chars))
            .collect();
        Ok(Layout::new(image.width, image.height, lines))
    }
}

/// Reads one model, saying which one it was if it will not load.
fn load_model(kind: ModelKind, bytes: Vec<u8>) -> Result<rten::Model, OcrError> {
    rten::Model::load(bytes).map_err(|error| OcrError::ModelLoad {
        model: kind,
        source: Box::new(error),
    })
}

fn convert_line(line: &TextLine, include_chars: bool) -> Line {
    Line {
        text: line.to_string(),
        bbox: convert_rect(line.bounding_rect()),
        rotated_box: convert_rotated_rect(line.rotated_rect()),
        words: line
            .words()
            .map(|word| convert_word(&word, include_chars))
            .collect(),
        // The engine keeps its per-step probabilities to itself: nothing it
        // hands back carries a score, at any granularity.
        confidence: None,
    }
}

fn convert_word(word: &TextWord<'_>, include_chars: bool) -> Word {
    Word {
        text: word.to_string(),
        bbox: convert_rect(word.bounding_rect()),
        rotated_box: convert_rotated_rect(word.rotated_rect()),
        chars: if include_chars {
            word.chars()
                .iter()
                .map(|character| Char {
                    text: character.char.to_string(),
                    bbox: convert_rect(character.rect),
                    confidence: None,
                })
                .collect()
        } else {
            Vec::new()
        },
        confidence: None,
    }
}

/// Widens the engine's whole-pixel rectangle into the layout model's.
fn convert_rect(rect: PixelRect<i32>) -> Rect {
    Rect::new(
        rect.left() as f32,
        rect.top() as f32,
        rect.width() as f32,
        rect.height() as f32,
    )
}

/// Restates the engine's oriented rectangle in the layout model's terms.
///
/// The engine describes orientation with a unit "up" axis, and measures the
/// box across that axis (its width) and along it (its height). The layout
/// model instead names the angle from the positive x axis to the box's width
/// axis, positive clockwise on screen.
///
/// The width axis is the up axis turned a quarter turn clockwise on screen,
/// which is the direction the text reads in: upright text has an up axis of
/// `(0, -1)`, since y grows downwards, and so an angle of zero.
fn convert_rotated_rect(rect: RotatedRect) -> RotatedBox {
    let (centre, up) = (rect.center(), rect.up_axis());
    let angle_deg = up.x.atan2(-up.y).to_degrees();
    RotatedBox::new(
        centre.x,
        centre.y,
        rect.width(),
        rect.height(),
        // A degenerate up axis normalises to nothing finite; call such a box
        // unrotated rather than poisoning the layout with a NaN.
        if angle_deg.is_finite() {
            angle_deg
        } else {
            0.0
        },
    )
}

#[cfg(test)]
mod tests {
    use rten_imageproc::{Point, Vec2};

    use super::*;

    /// Loose enough for `f32` trigonometry, tight enough to catch a wrong
    /// sign or a swapped axis.
    const EPSILON: f32 = 1e-4;

    /// The engine's oriented rectangle for text whose up axis points in the
    /// given screen direction.
    fn rect_facing(up_x: f32, up_y: f32) -> RotatedRect {
        RotatedRect::new(
            Point::from_yx(20.0, 10.0),
            Vec2::from_xy(up_x, up_y),
            30.0,
            8.0,
        )
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn upright_text_is_unrotated() {
        // y grows downwards, so the up axis of upright text points at -y.
        let box_ = convert_rotated_rect(rect_facing(0.0, -1.0));
        assert_eq!(box_.cx, 10.0);
        assert_eq!(box_.cy, 20.0);
        assert_eq!(box_.width, 30.0);
        assert_eq!(box_.height, 8.0);
        assert_close(box_.angle_deg, 0.0);
    }

    #[test]
    fn text_reading_downwards_is_a_quarter_turn_clockwise() {
        // Turning the page anticlockwise leaves the text reading downwards
        // and its up axis pointing at +x.
        assert_close(convert_rotated_rect(rect_facing(1.0, 0.0)).angle_deg, 90.0);
    }

    #[test]
    fn text_reading_upwards_is_a_quarter_turn_anticlockwise() {
        assert_close(
            convert_rotated_rect(rect_facing(-1.0, 0.0)).angle_deg,
            -90.0,
        );
    }

    #[test]
    fn upside_down_text_is_a_half_turn() {
        assert_close(convert_rotated_rect(rect_facing(0.0, 1.0)).angle_deg, 180.0);
    }

    #[test]
    fn a_small_tilt_keeps_its_sign() {
        // The up axis leaning to the right tilts the text clockwise.
        let leaning_right = convert_rotated_rect(rect_facing(0.1, -1.0)).angle_deg;
        assert!(
            leaning_right > 0.0 && leaning_right < 10.0,
            "expected a small clockwise angle, got {leaning_right}"
        );
        assert_close(
            convert_rotated_rect(rect_facing(-0.1, -1.0)).angle_deg,
            -leaning_right,
        );
    }

    #[test]
    fn corners_of_a_converted_box_match_the_engines() {
        // The two descriptions disagree about which corner comes first and
        // which way round they go, but they must agree on the four points.
        let rect = rect_facing(0.3, -1.0);
        let mut engine_corners: Vec<_> = rect.corners().map(|point| (point.x, point.y)).into();
        let mut converted: Vec<_> = convert_rotated_rect(rect).corners().into();
        for corners in [&mut engine_corners, &mut converted] {
            corners.sort_by(|a, b| a.partial_cmp(b).expect("corners are finite"));
        }
        for ((x, y), (want_x, want_y)) in converted.iter().zip(engine_corners.iter()) {
            assert_close(*x, *want_x);
            assert_close(*y, *want_y);
        }
    }

    #[test]
    fn a_degenerate_up_axis_gives_an_unrotated_box() {
        let box_ = convert_rotated_rect(rect_facing(0.0, 0.0));
        assert_eq!(box_.angle_deg, 0.0);
    }

    #[test]
    fn whole_pixel_rectangles_widen_to_floats() {
        let rect = convert_rect(PixelRect::from_tlhw(4, 2, 9, 7));
        assert_eq!(rect, Rect::new(2.0, 4.0, 7.0, 9.0));
    }

    #[test]
    fn characters_are_kept_by_default() {
        assert!(OcrOptions::default().include_chars);
    }

    #[test]
    fn an_image_with_no_pixels_is_rejected() {
        let image = PixelImage::new(0, 12, Channels::Rgb, &[]);
        assert!(matches!(
            image.to_source(),
            Err(OcrError::EmptyImage {
                width: 0,
                height: 12
            })
        ));
    }

    #[test]
    fn a_buffer_of_the_wrong_length_is_rejected() {
        let image = PixelImage::new(4, 2, Channels::Rgb, &[0; 16]);
        assert!(matches!(
            image.to_source(),
            Err(OcrError::PixelCount {
                expected: 24,
                actual: 16,
                ..
            })
        ));
    }

    #[test]
    fn a_buffer_of_the_right_length_is_accepted() {
        for (channels, len) in [
            (Channels::Gray, 8),
            (Channels::Rgb, 24),
            (Channels::Rgba, 32),
        ] {
            let data = vec![0; len];
            assert!(
                PixelImage::new(4, 2, channels, &data).to_source().is_ok(),
                "{channels} of {len} bytes should describe 4 by 2 pixels"
            );
        }
    }

    #[cfg(feature = "decode")]
    #[test]
    fn a_greyscale_png_decodes_to_one_channel_per_pixel() {
        let png = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/hello.png"
        ));
        let image = PixelImage::decode(png).expect("the fixture is a PNG");
        assert_eq!(image.channels, Channels::Gray);
        assert_eq!(
            image.data.len(),
            image.width as usize * image.height as usize
        );
        assert!(image.as_pixel_image().to_source().is_ok());
    }

    #[cfg(feature = "decode")]
    #[test]
    fn bytes_that_are_not_an_image_are_rejected() {
        assert!(matches!(
            PixelImage::decode(b"not an image"),
            Err(OcrError::Decode { .. })
        ));
    }

    /// One opaque black pixel as farbfeld: the magic, the width and height,
    /// and four sixteen-bit channels. Farbfeld is not one of the formats a
    /// build reads unless it is asked to, so it stands here for all of them.
    #[cfg(feature = "decode")]
    const FARBFELD_PIXEL: &[u8] =
        b"farbfeld\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\xff\xff";

    #[cfg(all(feature = "decode", not(feature = "decode-farbfeld")))]
    #[test]
    fn an_image_in_a_format_this_build_has_no_decoder_for_is_rejected() {
        assert!(matches!(
            PixelImage::decode(FARBFELD_PIXEL),
            Err(OcrError::Decode { .. })
        ));
    }

    #[cfg(feature = "decode-farbfeld")]
    #[test]
    fn an_image_in_a_format_this_build_was_asked_for_is_read() {
        let image = PixelImage::decode(FARBFELD_PIXEL).expect("farbfeld is compiled in");
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.channels, Channels::Rgba);
    }
}
