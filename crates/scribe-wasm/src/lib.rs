//! WebAssembly bindings for scribe.
//!
//! A thin translation layer: JavaScript values in, scribe-core calls,
//! JavaScript values out. Any behaviour worth testing belongs in the core.
//!
//! A caller supplies the model bytes, since nothing here fetches anything,
//! builds an [`Engine`] once and reads as many images with it as it likes.
//! What comes back is a layout: a plain object that survives
//! `JSON.stringify`, can be kept, and can be handed to [`render`] later
//! without the models being present at all.

mod convert;
mod types;

pub use types::{
    JsChannels, JsLayout, JsOcrOptions, JsOptionSpecs, JsRenderOptions, JsRendered, JsSchema,
    JsSourceImage,
};

use scribe_core::image_source::ImageSource;
use scribe_core::layout::Layout;
use scribe_core::ocr::{Models, PixelImage};
use scribe_core::render::{OptionKind, OptionValue, Registry, Renderer, list_templates, registry};
use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsError, JsValue};

/// A loaded pair of models, ready to read images.
///
/// Building one is expensive and reading an image is not, so build the engine
/// once and analyse as many images with it as you like. Call `free()` when
/// you are done with it: the models are large and nothing else will let go of
/// them.
#[wasm_bindgen]
pub struct Engine {
    /// The core engine everything here defers to.
    engine: scribe_core::ocr::Engine,
}

#[wasm_bindgen]
impl Engine {
    /// Loads the detection and recognition models and starts an engine that
    /// runs them.
    ///
    /// The bytes are copied, so the buffers they came from may be reused.
    ///
    /// # Errors
    ///
    /// Throws if the options are not of the kinds they should be, or if
    /// either model cannot be loaded.
    #[wasm_bindgen(constructor)]
    pub fn new(
        detection_model: &[u8],
        recognition_model: &[u8],
        options: Option<JsOcrOptions>,
    ) -> Result<Engine, JsError> {
        let models = Models::new(detection_model.to_vec(), recognition_model.to_vec());
        let options = convert::ocr_options(options.map(JsValue::from))?;
        let engine = scribe_core::ocr::Engine::new(&models, options)
            .map_err(|error| convert::exception(&error))?;
        Ok(Self { engine })
    }

    /// Reads the text in a buffer of pixels, row by row from the top, each
    /// row left to right, with the channels of a pixel adjacent.
    ///
    /// This is what a `<canvas>` gives out: pass its `ImageData` width,
    /// height and `data` with four channels.
    ///
    /// # Errors
    ///
    /// Throws if the channel count is not 1, 3 or 4, if the buffer is not as
    /// long as an image of that size needs, or if the engine fails.
    #[wasm_bindgen(js_name = analyzePixels)]
    pub fn analyze_pixels(
        &self,
        width: u32,
        height: u32,
        channels: JsChannels,
        data: &[u8],
    ) -> Result<JsLayout, JsError> {
        let channels = convert::channels(&JsValue::from(channels))?;
        let image = PixelImage::new(width, height, channels, data);
        let layout = self
            .engine
            .analyze(&image)
            .map_err(|error| convert::exception(&error))?;
        Ok(convert::to_js(&layout)?.unchecked_into())
    }

    /// Reads the text in an encoded image — a PNG, a JPEG, a WebP — decoding
    /// it first.
    ///
    /// # Errors
    ///
    /// Throws if the bytes are not an image this build can decode, or if the
    /// engine fails.
    #[cfg(feature = "decode")]
    #[wasm_bindgen(js_name = analyzeEncoded)]
    pub fn analyze_encoded(&self, bytes: &[u8]) -> Result<JsLayout, JsError> {
        let decoded = PixelImage::decode(bytes).map_err(|error| convert::exception(&error))?;
        let layout = self
            .engine
            .analyze(&decoded.as_pixel_image())
            .map_err(|error| convert::exception(&error))?;
        Ok(convert::to_js(&layout)?.unchecked_into())
    }
}

/// Turns a layout into a document of the named format.
///
/// No model is needed here: a layout kept from an earlier read renders as
/// well as one just produced. The image is what the layout was read from; a
/// renderer may embed it, link to it, or ignore it entirely, so it can be
/// left out when the format does not want it. Its dimensions then come from
/// the layout.
///
/// # Errors
///
/// Throws if the layout is not a layout document, if there is no format of
/// that name, if the options are not ones the format takes, or if the
/// document cannot be written.
#[wasm_bindgen]
pub fn render(
    layout: JsLayout,
    format: &str,
    options: Option<JsRenderOptions>,
    image: Option<JsSourceImage>,
) -> Result<JsRendered, JsError> {
    let layout: Layout = serde_wasm_bindgen::from_value(layout.into())
        .map_err(|error| JsError::new(&format!("that is not a layout document: {error}")))?;
    let registry = registry();
    let renderer = choose(&registry, format)?;
    let options = convert::options(options.map(JsValue::from))?;
    let described = convert::image(image.map(JsValue::from))?;
    let source = match &described {
        Some(image) => image.source(),
        None => ImageSource::new(layout.image.width, layout.image.height),
    };
    let output = renderer
        .render(&layout, &source, &options)
        .map_err(|error| convert::exception(&error))?;
    Ok(convert::rendered(&output))
}

/// The names of every format this build can render, in alphabetical order.
#[wasm_bindgen]
pub fn formats() -> Vec<String> {
    registry().names().into_iter().map(str::to_string).collect()
}

/// What one format can be told, ready to be offered to somebody choosing.
///
/// This is the whole of what a caller needs in order to build a form, a menu
/// or a listing for a format without knowing anything else about it.
///
/// # Errors
///
/// Throws if there is no format of that name.
#[wasm_bindgen(js_name = describeOptions)]
pub fn describe_options(format: &str) -> Result<JsOptionSpecs, JsError> {
    let registry = registry();
    let specs = choose(&registry, format)?.describe_options();
    let described: Vec<Spec<'_>> = specs
        .iter()
        .map(|spec| Spec {
            name: spec.name,
            kind: match spec.kind {
                OptionKind::Bool => "bool",
                OptionKind::Int => "int",
                OptionKind::Float => "float",
                OptionKind::Str => "str",
            },
            default: match &spec.default {
                OptionValue::Bool(value) => Default::Bool(*value),
                OptionValue::Int(value) => Default::Int(*value),
                OptionValue::Float(value) => Default::Float(*value),
                OptionValue::Str(text) => Default::Str(text),
            },
            help: spec.help,
            choices: spec.choices,
        })
        .collect();
    Ok(convert::to_js(&described)?.unchecked_into())
}

/// The names of the templates that ship with scribe, for the `template`
/// format's `template` option.
#[wasm_bindgen]
pub fn templates() -> Vec<String> {
    list_templates().into_iter().map(str::to_string).collect()
}

/// The JSON Schema describing the layout model.
///
/// Consumers can generate their own types from it, and a layout that has been
/// somewhere else and back can be validated against it before being rendered.
///
/// # Errors
///
/// Throws if the schema cannot be passed on, which it always can.
#[wasm_bindgen(js_name = layoutSchema)]
pub fn layout_schema() -> Result<JsSchema, JsError> {
    Ok(convert::to_js(&Layout::json_schema())?.unchecked_into())
}

/// The version of scribe these bindings were built from.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// One option a renderer takes, spelled the way JavaScript reads it.
#[derive(Serialize)]
struct Spec<'a> {
    /// The name it is set by.
    name: &'a str,
    /// The kind of value it takes.
    kind: &'static str,
    /// What it is when nobody sets it.
    default: Default<'a>,
    /// A sentence describing it.
    help: &'a str,
    /// The words it accepts, empty when any value of its kind will do.
    choices: &'a [&'a str],
}

/// An option's default, as the JavaScript type its kind belongs to.
#[derive(Serialize)]
#[serde(untagged)]
enum Default<'a> {
    /// True or false.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A number, whole or not.
    Float(f64),
    /// Text.
    Str(&'a str),
}

/// The renderer of that name, or a message listing the ones there are.
fn choose<'a>(registry: &'a Registry, name: &str) -> Result<&'a dyn Renderer, JsError> {
    registry.get(name).ok_or_else(|| {
        JsError::new(&format!(
            "there is no `{name}` format; scribe renders {}",
            registry.names().join(", ")
        ))
    })
}
