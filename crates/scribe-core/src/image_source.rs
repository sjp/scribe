//! What a renderer knows about the raster image a layout came from.
//!
//! Renderers differ in what they need: one embeds the original image, another
//! links to it, another ignores it entirely and emits text alone. This module
//! describes the source — its pixel dimensions, its media type and, when the
//! caller can provide them, its encoded bytes — so that each renderer can take
//! only what it uses.
//!
//! ```
//! use scribe_core::image_source::ImageSource;
//!
//! let png = b"\x89PNG\r\n\x1a\n";
//! let source = ImageSource::new(8, 4)
//!     .with_mime("image/png")
//!     .with_bytes(png)
//!     .with_href("diagram.png");
//! assert_eq!(
//!     source.data_uri().as_deref(),
//!     Some("data:image/png;base64,iVBORw0KGgo=")
//! );
//! ```

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

/// The raster a layout was read from, as much of it as the caller can offer.
///
/// The dimensions are always known, since the layout's coordinates are in
/// them. Everything else is optional: a renderer that embeds the image needs
/// [`bytes`](Self::bytes) and [`mime`](Self::mime), one that links to it needs
/// [`href`](Self::href), and one that emits text alone needs neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageSource<'a> {
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// The media type of the encoded image, such as `image/png`.
    pub mime: Option<&'a str>,
    /// The encoded image exactly as it arrived, if the caller kept it.
    pub bytes: Option<&'a [u8]>,
    /// A path or URL the output can point at instead of carrying the image.
    pub href: Option<&'a str>,
}

impl<'a> ImageSource<'a> {
    /// Describes an image of the given pixel size and nothing more.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mime: None,
            bytes: None,
            href: None,
        }
    }

    /// Adds the media type of the encoded image.
    #[must_use]
    pub fn with_mime(mut self, mime: &'a str) -> Self {
        self.mime = Some(mime);
        self
    }

    /// Adds the encoded image itself.
    #[must_use]
    pub fn with_bytes(mut self, bytes: &'a [u8]) -> Self {
        self.bytes = Some(bytes);
        self
    }

    /// Adds a path or URL the image can be referenced by.
    #[must_use]
    pub fn with_href(mut self, href: &'a str) -> Self {
        self.href = Some(href);
        self
    }

    /// The image as a `data:` URI, ready to be embedded in an output.
    ///
    /// This is `None` unless both the bytes and the media type are known,
    /// since neither one alone says what the other would mean. The bytes are
    /// encoded as standard base64 with padding, which every browser accepts.
    pub fn data_uri(&self) -> Option<String> {
        let (mime, bytes) = (self.mime?, self.bytes?);
        Some(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest PNG that decoders accept: one opaque black pixel.
    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a,
        0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe5, 0x27, 0xde, 0xfc, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_bare_source_knows_only_its_size() {
        let source = ImageSource::new(640, 480);
        assert_eq!((source.width, source.height), (640, 480));
        assert_eq!(source.mime, None);
        assert_eq!(source.bytes, None);
        assert_eq!(source.href, None);
        assert_eq!(source.data_uri(), None);
    }

    #[test]
    fn a_png_encodes_as_a_data_uri() {
        let source = ImageSource::new(1, 1)
            .with_mime("image/png")
            .with_bytes(PIXEL_PNG);
        let uri = source.data_uri().expect("the bytes and the type are known");
        assert_eq!(
            uri,
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgAB5Sfe/AAAAABJRU5ErkJggg=="
        );
        assert_eq!(
            BASE64
                .decode(uri.rsplit_once(',').expect("the URI has a comma").1)
                .expect("the payload is base64"),
            PIXEL_PNG
        );
    }

    #[test]
    fn bytes_without_a_media_type_make_no_data_uri() {
        assert_eq!(
            ImageSource::new(1, 1).with_bytes(PIXEL_PNG).data_uri(),
            None
        );
        assert_eq!(
            ImageSource::new(1, 1).with_mime("image/png").data_uri(),
            None
        );
    }

    #[test]
    fn an_href_is_carried_alongside_the_bytes() {
        let source = ImageSource::new(2, 3)
            .with_mime("image/jpeg")
            .with_bytes(b"jpeg")
            .with_href("photos/holiday.jpg");
        assert_eq!(source.href, Some("photos/holiday.jpg"));
        assert_eq!(
            source.data_uri().as_deref(),
            Some("data:image/jpeg;base64,anBlZw==")
        );
    }
}
