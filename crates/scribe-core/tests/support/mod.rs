//! The fixture images and the layouts read from them.
//!
//! Every fixture is described here rather than being an opaque file, so that
//! the whole set can be drawn again from nothing but this repository and a
//! font: setting `UPDATE_FIXTURES=1` redraws the images and, with the models
//! named in the environment, reads each one and writes the layout beside it.
//! Ordinary test runs read the committed files and need neither a font nor a
//! model.

use std::path::{Path, PathBuf};
use std::sync::Once;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use image::ImageEncoder as _;
use scribe_core::layout::Layout;
use scribe_core::ocr::{Channels, Engine, Models, OcrOptions, PixelImage};

/// Set this to redraw the images and read the layouts again.
pub const UPDATE_VARIABLE: &str = "UPDATE_FIXTURES";

/// Set this to the path of the text detection model.
pub const DETECTION_VARIABLE: &str = "SCRIBE_DETECTION_MODEL";

/// Set this to the path of the text recognition model.
pub const RECOGNITION_VARIABLE: &str = "SCRIBE_RECOGNITION_MODEL";

/// Set this to a TrueType font to draw the fixtures with, when none of the
/// places [`FONT_PATHS`] looks in holds one.
pub const FONT_VARIABLE: &str = "SCRIBE_FIXTURE_FONT";

/// Where the font is looked for when the environment does not say.
///
/// DejaVu Sans is the one the committed images were drawn with; another font
/// draws readable fixtures too, but every layout then has to be read again.
pub const FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/local/share/fonts/DejaVuSans.ttf",
    "/Library/Fonts/DejaVuSans.ttf",
];

/// How many times over, in each direction, a turned line of text is sampled
/// as it is drawn.
const SUPERSAMPLE: u8 = 3;

/// How good a JPEG the photograph-like fixtures are kept as.
///
/// High enough that the text survives the compression, low enough that the
/// artefacts around it are the ones a real photograph would carry.
const JPEG_QUALITY: u8 = 88;

/// One line of text drawn into a fixture.
pub struct Text {
    /// What it says.
    pub text: &'static str,
    /// The height of an em in pixels.
    pub size: f32,
    /// Where its baseline starts, before it is turned, to the whole pixel.
    pub at: (f32, f32),
    /// How far it is turned about that point, in degrees, clockwise as seen
    /// on screen.
    pub angle_deg: f32,
}

/// What a fixture looks like: what lies under its text and how it is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Black text on white paper, kept as a greyscale PNG, as a screenshot
    /// or a scan gives.
    Paper,
    /// Text over a wash of colour, kept as a JPEG, as a photograph gives.
    Photograph,
}

impl Kind {
    /// The extension a fixture of this kind is named with.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Paper => "png",
            Self::Photograph => "jpg",
        }
    }

    /// The media type of a fixture of this kind.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Paper => "image/png",
            Self::Photograph => "image/jpeg",
        }
    }

    /// How the pixels of a fixture of this kind reach the engine.
    fn channels(self) -> Channels {
        match self {
            Self::Paper => Channels::Gray,
            Self::Photograph => Channels::Rgb,
        }
    }
}

/// One image the tests run against, and everything needed to draw it again.
pub struct Fixture {
    /// The name it is known by, which is its file name without the
    /// extension.
    pub stem: &'static str,
    /// What it looks like and how it is kept.
    pub kind: Kind,
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// The lines of text drawn into it, if any.
    pub text: &'static [Text],
}

/// The whole fixture set, each one covering something a renderer has to cope
/// with.
pub const FIXTURES: &[Fixture] = &[
    // One short line, level and large: the simplest thing that can be read.
    Fixture {
        stem: "hello",
        kind: Kind::Paper,
        width: 284,
        height: 96,
        text: &[Text {
            text: "Hello World",
            size: 48.0,
            at: (24.0, 62.0),
            angle_deg: 0.0,
        }],
    },
    // Several lines in three sizes, as a page of prose with a heading and a
    // footnote has.
    Fixture {
        stem: "paragraph",
        kind: Kind::Paper,
        width: 480,
        height: 280,
        text: &[
            Text {
                text: "Reading Machines",
                size: 34.0,
                at: (32.0, 56.0),
                angle_deg: 0.0,
            },
            Text {
                text: "The quick brown fox jumps",
                size: 22.0,
                at: (32.0, 110.0),
                angle_deg: 0.0,
            },
            Text {
                text: "over the lazy dog, and does",
                size: 22.0,
                at: (32.0, 146.0),
                angle_deg: 0.0,
            },
            Text {
                text: "it again on the next line.",
                size: 22.0,
                at: (32.0, 182.0),
                angle_deg: 0.0,
            },
            Text {
                text: "Printed in 1996 at Leyden.",
                size: 14.0,
                at: (32.0, 240.0),
                angle_deg: 0.0,
            },
        ],
    },
    // A line off level and a line on its side, which are the two ways text
    // stops being axis aligned. The engine reads the tilted one and gives it
    // a turned box; it makes very little of the upright one, which is worth
    // having as well, since a renderer meets that kind of layout too.
    Fixture {
        stem: "rotated",
        kind: Kind::Paper,
        width: 420,
        height: 320,
        text: &[
            Text {
                text: "Tilted line",
                size: 34.0,
                at: (40.0, 90.0),
                angle_deg: 15.0,
            },
            Text {
                text: "Sideways",
                size: 34.0,
                at: (370.0, 150.0),
                angle_deg: 90.0,
            },
        ],
    },
    // Two small pieces of text at opposite corners of a photograph, with
    // most of the image holding nothing to read.
    Fixture {
        stem: "sparse",
        kind: Kind::Photograph,
        width: 480,
        height: 360,
        text: &[
            Text {
                text: "North gate",
                size: 26.0,
                at: (28.0, 52.0),
                angle_deg: 0.0,
            },
            Text {
                text: "South pier",
                size: 26.0,
                at: (300.0, 330.0),
                angle_deg: 0.0,
            },
        ],
    },
    // Nothing to read at all, which every renderer still has to render.
    Fixture {
        stem: "blank",
        kind: Kind::Paper,
        width: 220,
        height: 140,
        text: &[],
    },
];

impl Fixture {
    /// The name of the image file, such as `hello.png`.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.stem, self.kind.extension())
    }

    /// Where the image is.
    pub fn image_path(&self) -> PathBuf {
        directory().join(self.file_name())
    }

    /// Where the layout read from the image is.
    pub fn layout_path(&self) -> PathBuf {
        directory().join(format!("{}.layout.json", self.stem))
    }

    /// The encoded image, as committed.
    pub fn image_bytes(&self) -> Vec<u8> {
        let path = self.image_path();
        std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()))
    }

    /// The layout read from the image, as committed.
    pub fn layout(&self) -> Layout {
        let path = self.layout_path();
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
        Layout::from_json(&json)
            .unwrap_or_else(|error| panic!("{} is not a layout: {error}", path.display()))
    }

    /// Draws the image and encodes it the way the committed one is encoded.
    pub fn draw(&self, font: &FontVec) -> Vec<u8> {
        let mut ink = Ink::new(self.width, self.height);
        for line in self.text {
            ink.write(font, line);
        }
        let mut pixels = image::RgbImage::new(self.width, self.height);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let under = background(self.kind, x, y, self.width, self.height);
            let lit = 1.0 - ink.at(x, y);
            *pixel = image::Rgb(under.map(|channel| (channel as f32 * lit).round() as u8));
        }
        encode(self.kind, &pixels)
    }

    /// Reads the committed image with the engine, as the pipeline does.
    pub fn analyze(&self, engine: &Engine) -> Layout {
        let decoded = image::load_from_memory(&self.image_bytes())
            .unwrap_or_else(|error| panic!("{} cannot be decoded: {error}", self.file_name()));
        let (width, height) = (decoded.width(), decoded.height());
        let channels = self.kind.channels();
        let data = match channels {
            Channels::Gray => decoded.into_luma8().into_raw(),
            _ => decoded.into_rgb8().into_raw(),
        };
        engine
            .analyze(&PixelImage::new(width, height, channels, &data))
            .unwrap_or_else(|error| panic!("{} cannot be read: {error}", self.file_name()))
    }
}

/// Writes the fixtures, when this run was asked to and before anything has
/// read them.
///
/// Every test begins with this, so that one run with `UPDATE_FIXTURES=1`
/// draws the images, reads them with the models named in the environment and
/// writes the layouts, whatever order the tests happen to run in. A run that
/// was not asked to does nothing at all.
///
/// # Panics
///
/// Panics if there is no font to draw with or no model to read with. Both are
/// found before anything is written, since a run that wrote only half the set
/// would leave the layouts describing images that no longer exist.
pub fn prepare() {
    static WRITTEN: Once = Once::new();
    WRITTEN.call_once(|| {
        if !updating() {
            return;
        }
        let (font, engine) = (font(), engine());
        for fixture in FIXTURES {
            write(&fixture.image_path(), &fixture.draw(&font));
        }
        for fixture in FIXTURES {
            let mut json = fixture
                .analyze(&engine)
                .to_json_pretty()
                .expect("a layout is JSON");
            json.push('\n');
            write(&fixture.layout_path(), json.as_bytes());
        }
    });
}

/// Replaces a fixture with what this run drew or read.
fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes)
        .unwrap_or_else(|error| panic!("{} cannot be written: {error}", path.display()));
}

/// Where the fixtures live.
pub fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Whether this run was asked to write the fixtures rather than read them.
pub fn updating() -> bool {
    std::env::var_os(UPDATE_VARIABLE).is_some_and(|value| !value.is_empty() && value != "0")
}

/// The font the fixtures are drawn with.
///
/// # Panics
///
/// Panics if no font can be found, naming the variable that says where one
/// is. Only a run that is writing the fixtures needs one.
pub fn font() -> FontVec {
    let from_environment = std::env::var_os(FONT_VARIABLE).map(PathBuf::from);
    let candidates = from_environment
        .into_iter()
        .chain(FONT_PATHS.iter().map(PathBuf::from));
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            return FontVec::try_from_vec(bytes)
                .unwrap_or_else(|error| panic!("{} is not a font: {error}", path.display()));
        }
    }
    panic!("no font to draw the fixtures with; set {FONT_VARIABLE} to the path of one");
}

/// The engine the layouts are read with.
///
/// # Panics
///
/// Panics if the models are not where the environment says, naming the
/// variables that say where they are. Only a run that is writing the
/// fixtures needs them.
pub fn engine() -> Engine {
    let read = |variable: &str| {
        let path = PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| {
            panic!("writing the fixtures needs {variable} to name a model file")
        }));
        std::fs::read(&path).unwrap_or_else(|error| {
            panic!("{variable} points at {}, which {error}", path.display())
        })
    };
    let models = Models::new(read(DETECTION_VARIABLE), read(RECOGNITION_VARIABLE));
    Engine::new(models, OcrOptions::default()).expect("the models load")
}

/// How much of each pixel of an image the text drawn into it covers, from
/// none of it to all of it.
struct Ink {
    /// The width of the image in pixels.
    width: u32,
    /// The height of the image in pixels.
    height: u32,
    /// One coverage value per pixel, row by row from the top.
    coverage: Vec<f32>,
}

impl Ink {
    /// No ink at all over an image of the given size.
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0.0; width as usize * height as usize],
        }
    }

    /// How much of a pixel is covered.
    fn at(&self, x: u32, y: u32) -> f32 {
        self.coverage[y as usize * self.width as usize + x as usize]
    }

    /// Covers a pixel at least this much, the ink of two glyphs that meet
    /// being no darker than the darker of them.
    fn cover(&mut self, x: i32, y: i32, coverage: f32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let cell = &mut self.coverage[y as usize * self.width as usize + x as usize];
        *cell = cell.max(coverage.clamp(0.0, 1.0));
    }

    /// Draws one line of text.
    ///
    /// A line that is not turned is laid down glyph by glyph, so that its
    /// pixels are exactly what the rasteriser produced. A turned one is
    /// rasterised several times larger, level, and then sampled back down
    /// through the turn, which leaves its edges as smooth as a level line's
    /// rather than as ragged as resampling one pixel grid onto another.
    fn write(&mut self, font: &FontVec, line: &Text) {
        if line.angle_deg == 0.0 {
            let mask = Mask::rasterize(font, line, line.size);
            for (x, y, coverage) in mask.pixels() {
                self.cover(
                    line.at.0 as i32 + mask.left + x as i32,
                    line.at.1 as i32 + mask.top + y as i32,
                    coverage,
                );
            }
            return;
        }

        let magnified = f32::from(SUPERSAMPLE);
        let mask = Mask::rasterize(font, line, line.size * magnified);
        let (sin, cos) = line.angle_deg.to_radians().sin_cos();
        let turn = |x: f32, y: f32| (line.at.0 + x * cos - y * sin, line.at.1 + x * sin + y * cos);
        let corners = [
            (mask.left as f32, mask.top as f32),
            (mask.right() as f32, mask.top as f32),
            (mask.right() as f32, mask.bottom() as f32),
            (mask.left as f32, mask.bottom() as f32),
        ]
        .map(|(x, y)| turn(x / magnified, y / magnified));
        let horizontal = corners.iter().map(|(x, _)| *x);
        let vertical = corners.iter().map(|(_, y)| *y);
        let (min_x, max_x) = span(horizontal);
        let (min_y, max_y) = span(vertical);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                // Take several points across this pixel, turn each of them
                // back into the level text, and average the ink there.
                let mut coverage = 0.0;
                for down in 0..SUPERSAMPLE {
                    for across in 0..SUPERSAMPLE {
                        let offset = |step: u8| (f32::from(step) + 0.5) / magnified;
                        let (dx, dy) = (
                            x as f32 + offset(across) - line.at.0,
                            y as f32 + offset(down) - line.at.1,
                        );
                        let level = (dx * cos + dy * sin, -dx * sin + dy * cos);
                        coverage += mask.sample(
                            level.0 * magnified - mask.left as f32,
                            level.1 * magnified - mask.top as f32,
                        );
                    }
                }
                self.cover(x, y, coverage / (magnified * magnified));
            }
        }
    }
}

/// The whole numbers spanning a run of coordinates, from the one below the
/// least to the one above the greatest.
fn span(coordinates: impl Iterator<Item = f32>) -> (i32, i32) {
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for coordinate in coordinates {
        low = low.min(coordinate);
        high = high.max(coordinate);
    }
    (low.floor() as i32, high.ceil() as i32)
}

/// One line of text rasterised level, with its own bounds.
///
/// Its coordinates are in the pixels it was rasterised at, which are the
/// image's own for a level line and several to the image's one for a turned
/// line.
struct Mask {
    /// Where the left edge of the rasterised text is, relative to where the
    /// baseline starts.
    left: i32,
    /// Where the top edge is, relative to the same point.
    top: i32,
    /// The width of the rasterised text in pixels.
    width: u32,
    /// The height of the rasterised text in pixels.
    height: u32,
    /// One coverage value per pixel, row by row from the top.
    coverage: Vec<f32>,
}

impl Mask {
    /// Rasterises a line of text at the given size, with its pen starting at
    /// the origin and its baseline level.
    fn rasterize(font: &FontVec, line: &Text, size: f32) -> Self {
        let scale = PxScale::from(size);
        let scaled = font.as_scaled(scale);
        let mut outlines = Vec::new();
        let mut caret = 0.0;
        let mut previous = None;
        for character in line.text.chars() {
            let glyph = font.glyph_id(character);
            if let Some(previous) = previous {
                caret += scaled.kern(previous, glyph);
            }
            previous = Some(glyph);
            if let Some(outline) =
                font.outline_glyph(glyph.with_scale_and_position(scale, point(caret, 0.0)))
            {
                outlines.push(outline);
            }
            caret += scaled.h_advance(glyph);
        }

        if outlines.is_empty() {
            return Self {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                coverage: Vec::new(),
            };
        }

        let bounds = outlines.iter().map(|outline| outline.px_bounds());
        let left = bounds
            .clone()
            .fold(f32::MAX, |low, rect| low.min(rect.min.x))
            .floor() as i32;
        let top = bounds
            .clone()
            .fold(f32::MAX, |low, rect| low.min(rect.min.y))
            .floor() as i32;
        let right = bounds
            .clone()
            .fold(f32::MIN, |high, rect| high.max(rect.max.x))
            .ceil() as i32;
        let bottom = bounds
            .fold(f32::MIN, |high, rect| high.max(rect.max.y))
            .ceil() as i32;
        let (width, height) = ((right - left).max(0) as u32, (bottom - top).max(0) as u32);

        let mut mask = Self {
            left,
            top,
            width,
            height,
            coverage: vec![0.0; width as usize * height as usize],
        };
        for outline in &outlines {
            let origin = outline.px_bounds().min;
            let (offset_x, offset_y) = (origin.x as i32 - left, origin.y as i32 - top);
            outline.draw(|x, y, coverage| {
                let (x, y) = (offset_x + x as i32, offset_y + y as i32);
                if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
                    return;
                }
                let cell = &mut mask.coverage[y as usize * width as usize + x as usize];
                *cell = cell.max(coverage);
            });
        }
        mask
    }

    /// Where the right edge of the rasterised text is, relative to where the
    /// baseline starts.
    fn right(&self) -> i32 {
        self.left + self.width as i32
    }

    /// Where the bottom edge is, relative to the same point.
    fn bottom(&self) -> i32 {
        self.top + self.height as i32
    }

    /// Every pixel of the mask, with how much of it is covered.
    fn pixels(&self) -> impl Iterator<Item = (u32, u32, f32)> {
        self.coverage
            .iter()
            .enumerate()
            .map(|(index, coverage)| {
                let index = index as u32;
                (index % self.width, index / self.width, *coverage)
            })
            .filter(|(_, _, coverage)| *coverage > 0.0)
    }

    /// How much ink is at a point of the mask, between its pixels as well as
    /// on them, and none at all outside it.
    fn sample(&self, x: f32, y: f32) -> f32 {
        let (x, y) = (x - 0.5, y - 0.5);
        let (left, top) = (x.floor(), y.floor());
        let (fraction_x, fraction_y) = (x - left, y - top);
        let at = |x: f32, y: f32| {
            if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
                return 0.0;
            }
            self.coverage[y as usize * self.width as usize + x as usize]
        };
        let top_row = at(left, top) * (1.0 - fraction_x) + at(left + 1.0, top) * fraction_x;
        let bottom_row =
            at(left, top + 1.0) * (1.0 - fraction_x) + at(left + 1.0, top + 1.0) * fraction_x;
        top_row * (1.0 - fraction_y) + bottom_row * fraction_y
    }
}

/// The colour under the text at one pixel.
fn background(kind: Kind, x: u32, y: u32, width: u32, height: u32) -> [u8; 3] {
    match kind {
        Kind::Paper => [255; 3],
        Kind::Photograph => {
            // Broad bands of light and shade, a brighter patch off centre
            // and a little grain: nothing a recogniser should read, but
            // enough that the detector has to tell text from texture.
            let (across, down) = (x as f32 / width as f32, y as f32 / height as f32);
            let bands = 34.0 * (across * 2.2 + down * 1.3).sin();
            let patch = 46.0 * (-9.0 * ((across - 0.68).powi(2) + (down - 0.3).powi(2))).exp();
            let wash = 178.0 + bands + patch + 9.0 * grain(x, y);
            let shade = |tint: f32| (wash + tint).clamp(0.0, 255.0) as u8;
            [shade(9.0), shade(1.0), shade(-12.0)]
        }
    }
}

/// A value from -0.5 to 0.5 that looks random but is the same every time.
fn grain(x: u32, y: u32) -> f32 {
    let mut hash = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^= hash >> 16;
    hash as f32 / u32::MAX as f32 - 0.5
}

/// Encodes the drawn pixels the way a fixture of this kind is kept.
fn encode(kind: Kind, pixels: &image::RgbImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    match kind {
        Kind::Paper => {
            let grey = image::DynamicImage::ImageRgb8(pixels.clone()).into_luma8();
            image::codecs::png::PngEncoder::new(&mut bytes)
                .write_image(
                    grey.as_raw(),
                    grey.width(),
                    grey.height(),
                    image::ExtendedColorType::L8,
                )
                .expect("a PNG can be written to memory");
        }
        Kind::Photograph => {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
                .write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .expect("a JPEG can be written to memory");
        }
    }
    bytes
}
