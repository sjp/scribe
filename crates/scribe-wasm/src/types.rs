//! The shape of scribe as JavaScript sees it.
//!
//! wasm-bindgen writes `scribe.d.ts` from the signatures in this crate, and a
//! plain object arriving from JavaScript is, as far as those signatures go,
//! `any`. The declarations below are copied into that file word for word and
//! the types beneath them stand in for the objects they describe, so a
//! TypeScript caller sees the layout model, the options a renderer takes and
//! the document a render produces.
//!
//! Every name here is the one the core already uses: the members of a layout
//! are spelled as they are in its JSON, and a renderer's options are spelled
//! as `describeOptions` reports them.

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** An axis-aligned rectangle in image pixels, y increasing downwards. */
export interface Rect {
    /** The x coordinate of the left edge. */
    x: number;
    /** The y coordinate of the top edge. */
    y: number;
    /** The width, extending to the right. */
    width: number;
    /** The height, extending downwards. */
    height: number;
}

/**
 * An oriented rectangle in image pixels: a box of `width` by `height` centred
 * on `(cx, cy)` and turned by `angle_deg`, clockwise-positive on screen.
 */
export interface RotatedBox {
    /** The x coordinate of the centre. */
    cx: number;
    /** The y coordinate of the centre. */
    cy: number;
    /** The extent along the box's width axis. */
    width: number;
    /** The extent along the box's height axis. */
    height: number;
    /** The rotation in degrees, normalised to `(-180, 180]`. */
    angle_deg: number;
}

/** A single character within a word. */
export interface Char {
    /** The character itself, as one grapheme cluster. */
    text: string;
    /** The axis-aligned bounds of the character. */
    bbox: Rect;
    /** How sure the recogniser is, from 0 to 1, when it says. */
    confidence?: number | null;
}

/** A word within a line. */
export interface Word {
    /** The text of the word. */
    text: string;
    /** The axis-aligned bounds of the word. */
    bbox: Rect;
    /** The oriented bounds of the word. */
    rotated_box: RotatedBox;
    /** The characters of the word, in reading order; may be empty. */
    chars: Char[];
    /** How sure the recogniser is, from 0 to 1, when it says. */
    confidence?: number | null;
}

/** A line of recognised text. */
export interface Line {
    /** The text of the line, ready to be used as-is by a renderer. */
    text: string;
    /** The axis-aligned bounds of the line. */
    bbox: Rect;
    /** The oriented bounds of the line. */
    rotated_box: RotatedBox;
    /** The words of the line, in reading order. */
    words: Word[];
    /** How sure the recogniser is, from 0 to 1, when it says. */
    confidence?: number | null;
}

/** The raster a layout describes. */
export interface ImageInfo {
    /** The width of the image in pixels. */
    width: number;
    /** The height of the image in pixels. */
    height: number;
}

/**
 * Everything scribe knows about the text in one image.
 *
 * Coordinates are image pixels with the origin at the top left. This is the
 * contract between recognition and rendering: keep one of these and you can
 * render it again later without the models.
 */
export interface Layout {
    /** The version of the model this document was written against. */
    version: number;
    /** The raster the coordinates refer to. */
    image: ImageInfo;
    /** The lines of text, in reading order. */
    lines: Line[];
}

/** How the engine is asked to read. */
export interface OcrOptions {
    /**
     * The characters the recognition model was trained on, in the order it
     * was trained on them. Leave unset unless the model is your own.
     */
    alphabet?: string;
    /** The only characters recognition may produce, if it should be restricted. */
    allowed_chars?: string;
    /**
     * How many candidate readings to keep alive. Slower than taking the
     * likeliest character at every step, which is what happens when this is
     * unset, and sometimes more accurate.
     */
    beam_width?: number;
    /**
     * Whether to keep the per-character boxes in the layout. They are the
     * bulk of a serialised layout. Defaults to true.
     */
    include_chars?: boolean;
}

/**
 * The options a renderer takes, by name.
 *
 * Which names mean anything depends on the format; ask `describeOptions`.
 * A value of the wrong kind is converted when it can be, so `"false"` and
 * `false` mean the same thing.
 */
export type RenderOptions = Record<string, string | number | boolean>;

/**
 * The image a layout was read from, as much of it as you can offer.
 *
 * A renderer that embeds the image needs `bytes` and `mime`, one that links
 * to it needs `href`, and one that emits text alone needs neither.
 */
export interface SourceImage {
    /** The width of the image in pixels. */
    width: number;
    /** The height of the image in pixels. */
    height: number;
    /** The media type of the encoded image, such as `image/png`. */
    mime?: string;
    /** The encoded image exactly as it arrived. */
    bytes?: Uint8Array;
    /** A path or URL the output can point at instead of carrying the image. */
    href?: string;
}

/** A rendered document. */
export interface Rendered {
    /** The document itself. */
    bytes: Uint8Array;
    /** The media type of the document, such as `image/svg+xml`. */
    mime: string;
    /** The document as text, absent when the renderer produced other bytes. */
    text?: string;
}

/** The kind of value an option takes. */
export type OptionKind = "bool" | "int" | "float" | "str";

/** One option a renderer takes. */
export interface OptionSpec {
    /** The name it is set by. */
    name: string;
    /** The kind of value it takes. */
    kind: OptionKind;
    /** What it is when nobody sets it. */
    default: string | number | boolean;
    /** A sentence describing it, fit to show in a help listing. */
    help: string;
    /** The words it accepts; empty when any value of its kind will do. */
    choices: string[];
}
"#;

#[wasm_bindgen]
unsafe extern "C" {
    /// A layout, as `analyzePixels` returns one and `render` takes one.
    #[wasm_bindgen(typescript_type = "Layout")]
    pub type JsLayout;

    /// How to run the engine.
    #[wasm_bindgen(typescript_type = "OcrOptions")]
    pub type JsOcrOptions;

    /// The number of colour channels a pixel buffer has.
    #[wasm_bindgen(typescript_type = "1 | 3 | 4")]
    pub type JsChannels;

    /// A renderer's options, by name.
    #[wasm_bindgen(typescript_type = "RenderOptions")]
    pub type JsRenderOptions;

    /// The image a layout was read from.
    #[wasm_bindgen(typescript_type = "SourceImage")]
    pub type JsSourceImage;

    /// A rendered document.
    #[wasm_bindgen(typescript_type = "Rendered")]
    pub type JsRendered;

    /// Everything one renderer can be told.
    #[wasm_bindgen(typescript_type = "OptionSpec[]")]
    pub type JsOptionSpecs;

    /// The JSON Schema of the layout model.
    #[wasm_bindgen(typescript_type = "object")]
    pub type JsSchema;
}
