//! Reading JavaScript values into what the core takes, and handing its
//! answers back.
//!
//! Nothing here decides anything. A member that is missing falls back to the
//! core's own default, and one that is the wrong kind becomes a message
//! naming it, so a caller is told which part of the object it passed was
//! wrong rather than watching a render quietly do something else.

use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use scribe_core::image_source::ImageSource;
use scribe_core::ocr::{Channels, DecodeMethod, OcrOptions};
use scribe_core::render::{OptionValue, Options, RenderOutput};
use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::{JsCast, JsError, JsValue};

use crate::types::JsRendered;

/// A core error as an exception carrying the whole of what went wrong.
///
/// The core reports a failure as a short sentence with its cause beneath it,
/// and JavaScript has no such chain, so the sentences are joined into the one
/// message an `Error` can hold.
pub fn exception(error: &dyn std::error::Error) -> JsError {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        message.push_str(": ");
        message.push_str(&error.to_string());
        cause = error.source();
    }
    JsError::new(&message)
}

/// A value as the plain object its JSON would describe.
///
/// Missing values are written as `null` rather than left undefined, so that
/// what JavaScript receives survives a round trip through `JSON.stringify`.
pub fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsError> {
    value
        .serialize(&Serializer::json_compatible())
        .map_err(|error| JsError::new(&format!("the result could not be passed on: {error}")))
}

/// The channel layout a count of channels stands for.
pub fn channels(count: &JsValue) -> Result<Channels, JsError> {
    match count.as_f64() {
        Some(1.0) => Ok(Channels::Gray),
        Some(3.0) => Ok(Channels::Rgb),
        Some(4.0) => Ok(Channels::Rgba),
        _ => Err(JsError::new(
            "the channel count should be 1 for greyscale, 3 for RGB or 4 for RGBA",
        )),
    }
}

/// How to run the engine, as the given object asks for it.
pub fn ocr_options(value: Option<JsValue>) -> Result<OcrOptions, JsError> {
    let defaults = OcrOptions::default();
    let Some(members) = Members::of("options", value)? else {
        return Ok(defaults);
    };
    Ok(OcrOptions {
        alphabet: members.text("alphabet")?,
        allowed_chars: members.text("allowed_chars")?,
        decode_method: match members.count("beam_width")? {
            Some(width) => DecodeMethod::BeamSearch { width },
            None => defaults.decode_method,
        },
        include_chars: members
            .flag("include_chars")?
            .unwrap_or(defaults.include_chars),
    })
}

/// The options a renderer is to be given, from an object of name and value.
///
/// Every number JavaScript has is a float, so one without a fractional part
/// is passed on as a whole number; a renderer that wanted a float gets it
/// back as one when it resolves its options.
pub fn options(value: Option<JsValue>) -> Result<Options, JsError> {
    let Some(members) = Members::of("options", value)? else {
        return Ok(Options::new());
    };
    let mut options = Options::new();
    for entry in Object::entries(&members.object).iter() {
        let entry: Array = entry.unchecked_into();
        let name = entry
            .get(0)
            .as_string()
            .expect("the keys of an object are strings");
        let value = option_value(&name, &entry.get(1))?;
        options.set(name, value);
    }
    Ok(options)
}

/// The image a layout was read from, or `None` if the caller described none.
pub fn image(value: Option<JsValue>) -> Result<Option<Image>, JsError> {
    let Some(members) = Members::of("image", value)? else {
        return Ok(None);
    };
    Ok(Some(Image {
        width: members.required_count("width")?,
        height: members.required_count("height")?,
        mime: members.text("mime")?,
        bytes: members.bytes("bytes")?,
        href: members.text("href")?,
    }))
}

/// A rendered document as the object `render` resolves to.
pub fn rendered(output: &RenderOutput) -> JsRendered {
    let result = Object::new();
    set(&result, "bytes", &Uint8Array::from(output.bytes.as_slice()));
    set(&result, "mime", &JsValue::from_str(&output.mime));
    if let Some(text) = output.as_str() {
        set(&result, "text", &JsValue::from_str(text));
    }
    result.unchecked_into()
}

/// The image a layout was read from, owning what JavaScript lent it.
pub struct Image {
    /// The width of the image in pixels.
    width: u32,
    /// The height of the image in pixels.
    height: u32,
    /// The media type of the encoded image.
    mime: Option<String>,
    /// The encoded image itself.
    bytes: Option<Vec<u8>>,
    /// What an output can point at instead of carrying the image.
    href: Option<String>,
}

impl Image {
    /// The same image as the core describes one.
    pub fn source(&self) -> ImageSource<'_> {
        ImageSource {
            width: self.width,
            height: self.height,
            mime: self.mime.as_deref(),
            bytes: self.bytes.as_deref(),
            href: self.href.as_deref(),
        }
    }
}

/// One object arriving from JavaScript, read a member at a time.
///
/// A member that is absent, `undefined` or `null` counts as unset, since
/// those are all the ways an optional field goes unwritten in JavaScript.
struct Members {
    /// What the object is called in a message, such as `image`.
    what: &'static str,
    /// The object itself.
    object: Object,
}

impl Members {
    /// The given value as an object, or `None` if there was none.
    fn of(what: &'static str, value: Option<JsValue>) -> Result<Option<Self>, JsError> {
        match value {
            None => Ok(None),
            Some(value) if value.is_undefined() || value.is_null() => Ok(None),
            Some(value) if value.is_object() => Ok(Some(Self {
                what,
                object: value.unchecked_into(),
            })),
            Some(_) => Err(JsError::new(&format!("`{what}` should be an object"))),
        }
    }

    /// The value of one member, or `None` if it was left unset.
    fn member(&self, name: &str) -> Option<JsValue> {
        Reflect::get(&self.object, &JsValue::from_str(name))
            .ok()
            .filter(|value| !value.is_undefined() && !value.is_null())
    }

    /// One member as text.
    fn text(&self, name: &str) -> Result<Option<String>, JsError> {
        match self.member(name) {
            None => Ok(None),
            Some(value) => value
                .as_string()
                .map(Some)
                .ok_or_else(|| self.wrong(name, "a string")),
        }
    }

    /// One member as true or false.
    fn flag(&self, name: &str) -> Result<Option<bool>, JsError> {
        match self.member(name) {
            None => Ok(None),
            Some(value) => value
                .as_bool()
                .map(Some)
                .ok_or_else(|| self.wrong(name, "true or false")),
        }
    }

    /// One member as a count: a whole number, no smaller than zero.
    fn count(&self, name: &str) -> Result<Option<u32>, JsError> {
        match self.member(name) {
            None => Ok(None),
            Some(value) => value
                .as_f64()
                .filter(|number| {
                    number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(number)
                })
                .map(|number| Some(number as u32))
                .ok_or_else(|| self.wrong(name, "a whole number, no smaller than zero")),
        }
    }

    /// One member as a count that has to be there.
    fn required_count(&self, name: &str) -> Result<u32, JsError> {
        self.count(name)?
            .ok_or_else(|| JsError::new(&format!("`{}.{name}` is missing", self.what)))
    }

    /// One member as bytes, from a `Uint8Array` or an `ArrayBuffer`.
    fn bytes(&self, name: &str) -> Result<Option<Vec<u8>>, JsError> {
        let Some(value) = self.member(name) else {
            return Ok(None);
        };
        if value.is_instance_of::<Uint8Array>() {
            Ok(Some(value.unchecked_ref::<Uint8Array>().to_vec()))
        } else if value.is_instance_of::<ArrayBuffer>() {
            Ok(Some(Uint8Array::new(&value).to_vec()))
        } else {
            Err(self.wrong(name, "a Uint8Array"))
        }
    }

    /// The message for a member that is not what it should be.
    fn wrong(&self, name: &str, expected: &str) -> JsError {
        JsError::new(&format!("`{}.{name}` should be {expected}", self.what))
    }
}

/// One option's value, read as whichever kind JavaScript wrote it in.
fn option_value(name: &str, value: &JsValue) -> Result<OptionValue, JsError> {
    if let Some(flag) = value.as_bool() {
        return Ok(OptionValue::Bool(flag));
    }
    if let Some(text) = value.as_string() {
        return Ok(OptionValue::Str(text));
    }
    if let Some(number) = value.as_f64() {
        let whole = i64::MIN as f64..=i64::MAX as f64;
        return Ok(if number.fract() == 0.0 && whole.contains(&number) {
            OptionValue::Int(number as i64)
        } else {
            OptionValue::Float(number)
        });
    }
    Err(JsError::new(&format!(
        "the `{name}` option should be a string, a number or a boolean"
    )))
}

/// Writes one member of an object being built here, which cannot fail.
fn set(object: &Object, name: &str, value: &JsValue) {
    Reflect::set(object, &JsValue::from_str(name), value)
        .expect("a new object takes the members it is given");
}
