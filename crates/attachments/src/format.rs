//! What an image really is (from its bytes, never its name), whether Cilica keeps it, and turning
//! it into the bytes that get stored: TIFF becomes PNG, HEIC goes through the system converter,
//! big photos can be shrunk. Everything here is CPU work; callers run it off the async runtime.

use std::io::Cursor;
use std::sync::Arc;

use cilica_domain::{AppError, AppResult};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

// ---------- Limits ---------- //

/// Largest file Cilica takes in, before any conversion.
pub const MAX_INPUT_BYTES: usize = 40 * 1024 * 1024;
/// Most pixels an image may have (width × height).
pub const MAX_PIXELS: u64 = 100_000_000;
/// Longest side an image may have, in pixels.
pub const MAX_SIDE: u32 = 20_000;
/// With "shrink large images" on, the longest side photos are shrunk to.
pub const SHRINK_TO: u32 = 2560;
/// JPEG quality used when a shrunk photo is re-encoded.
const JPEG_QUALITY: u8 = 85;
/// Memory a single decode may use (100 MP of RGBA8 fits; 16-bit giants do not).
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// Turns HEIC/HEIF bytes into JPEG bytes. Supplied by the app where the OS can do it (macOS).
pub type HeicConverter = Arc<dyn Fn(&[u8]) -> AppResult<Vec<u8>> + Send + Sync>;

// ---------- Formats ---------- //

/// The formats an attachment is stored as. The webview shows all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Avif,
    Bmp,
}

impl StoredFormat {
    /// Every stored format, for parsing names.
    pub const ALL: [StoredFormat; 6] = [Self::Png, Self::Jpeg, Self::Gif, Self::Webp, Self::Avif, Self::Bmp];

    /// The file extension attachments of this format get.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Avif => "avif",
            Self::Bmp => "bmp",
        }
    }

    /// The format an attachment extension stands for.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.extension() == ext)
    }

    /// The `image` crate's decoder for this format. `None` for AVIF, which Cilica reads the size
    /// of itself but cannot decode.
    fn decoder_format(self) -> Option<image::ImageFormat> {
        match self {
            Self::Png => Some(image::ImageFormat::Png),
            Self::Jpeg => Some(image::ImageFormat::Jpeg),
            Self::Gif => Some(image::ImageFormat::Gif),
            Self::Webp => Some(image::ImageFormat::WebP),
            Self::Bmp => Some(image::ImageFormat::Bmp),
            Self::Avif => None,
        }
    }
}

/// What the first bytes of a file say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sniffed {
    /// Kept as it is.
    Stored(StoredFormat),
    /// Converted to PNG.
    Tiff,
    /// Converted to JPEG by the [`HeicConverter`].
    Heif,
    /// Refused: SVG can carry scripts and links.
    Svg,
    /// Refused: anything else.
    Other,
}

impl Sniffed {
    /// Read the type from magic bytes. Names and Content-Type are never trusted.
    fn of(bytes: &[u8]) -> Self {
        // Formats infer knows by their signature
        if let Some(kind) = infer::get(bytes) {
            match kind.mime_type() {
                "image/png" => return Self::Stored(StoredFormat::Png),
                "image/jpeg" => return Self::Stored(StoredFormat::Jpeg),
                "image/gif" => return Self::Stored(StoredFormat::Gif),
                "image/webp" => return Self::Stored(StoredFormat::Webp),
                "image/avif" => return Self::Stored(StoredFormat::Avif),
                "image/bmp" => return Self::Stored(StoredFormat::Bmp),
                "image/tiff" => return Self::Tiff,
                "image/heif" => return Self::Heif,
                _ => {}
            }
        }

        // HEIF brands infer does not list, then SVG, which is text
        if is_heif_brand(bytes) {
            return Self::Heif;
        }
        if looks_like_svg(bytes) {
            return Self::Svg;
        }
        Self::Other
    }
}

/// An ISO media file whose brand is one of HEIF's (`heic`, `heix`, `hevc`, `mif1`…), and not AVIF.
fn is_heif_brand(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let size = (u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize).clamp(16, bytes.len());
    let brands: Vec<&[u8]> = std::iter::once(&bytes[8..12]).chain(bytes[16..size].chunks_exact(4)).collect();
    let heif = brands.iter().any(|b| matches!(*b, b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1"));
    let avif = brands.iter().any(|b| matches!(*b, b"avif" | b"avis"));
    heif && !avif
}

/// Text that opens an `<svg>` element near its start (after any XML prolog or comments).
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    head.trim_start_matches('\u{feff}').trim_start().starts_with('<') && head.contains("<svg")
}

// ---------- Preparation ---------- //

/// The bytes Cilica stores for one image, and what they are.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub format: StoredFormat,
    /// Size as shown, after EXIF rotation.
    pub width: u32,
    pub height: u32,
}

/// Checks, converts and optionally shrinks incoming image bytes.
pub struct Preparer<'a> {
    /// Shrink photos whose longest side is over [`SHRINK_TO`].
    pub shrink_large: bool,
    /// Turns HEIC into JPEG; `None` where the OS cannot.
    pub heic: Option<&'a HeicConverter>,
}

impl Preparer<'_> {
    /// Turn raw bytes into what gets stored, or say for people why they can't be.
    pub fn prepare(&self, bytes: Vec<u8>) -> AppResult<Prepared> {
        // Size first, so nothing big gets parsed
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(AppError::invalid("That image is larger than 40 MB. Make it smaller and try again."));
        }

        // Keep, convert or refuse by what the bytes really are
        match Sniffed::of(&bytes) {
            Sniffed::Stored(format) => self.finish(bytes, format),
            Sniffed::Tiff => {
                let png = tiff_to_png(&bytes)?;
                self.finish(png, StoredFormat::Png)
            }
            Sniffed::Heif => {
                let Some(convert) = self.heic else {
                    return Err(AppError::invalid("Cilica can't open HEIC images on this computer. Save it as JPEG or PNG and try again."));
                };
                let jpeg = convert(&bytes)?;
                match Sniffed::of(&jpeg) {
                    Sniffed::Stored(format @ (StoredFormat::Jpeg | StoredFormat::Png)) => self.finish(jpeg, format),
                    _ => Err(AppError::invalid("Cilica couldn't convert that HEIC image. Save it as JPEG or PNG and try again.")),
                }
            }
            Sniffed::Svg => Err(AppError::invalid("Cilica doesn't keep SVG images. Save it as PNG or JPEG and try again.")),
            Sniffed::Other => Err(AppError::invalid("This file isn't an image Cilica can read. Use a PNG, JPEG, GIF, WebP, AVIF, BMP, TIFF or HEIC image.")),
        }
    }

    /// Read the size, enforce the limits, then shrink when asked and worth it.
    fn finish(&self, bytes: Vec<u8>, format: StoredFormat) -> AppResult<Prepared> {
        let (width, height) = shown_size(&bytes, format)?;
        check_size(width, height)?;

        // Shrink big still photos; on any trouble keep the original
        if self.shrink_large && width.max(height) > SHRINK_TO && can_shrink(&bytes, format) {
            match shrink(&bytes, format) {
                Ok(shrunk) => return Ok(shrunk),
                Err(error) => tracing::warn!(%error, "could not shrink an image; keeping it as it is"),
            }
        }
        Ok(Prepared { bytes, format, width, height })
    }
}

/// Refuse empty, oversized or absurdly large images.
fn check_size(width: u32, height: u32) -> AppResult<()> {
    if width == 0 || height == 0 {
        return Err(not_readable());
    }
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(AppError::invalid("That image is too big for Cilica (over 100 megapixels or 20,000 pixels on a side). Make it smaller and try again."));
    }
    Ok(())
}

/// The error for bytes that claim to be an image but don't decode.
fn not_readable() -> AppError {
    AppError::invalid("This file isn't an image Cilica can read.")
}

/// Decoding limits for every decode Cilica does.
fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// Width and height as shown: the header's size, swapped when EXIF turns the image a quarter.
fn shown_size(bytes: &[u8], format: StoredFormat) -> AppResult<(u32, u32)> {
    let Some(decoder_format) = format.decoder_format() else {
        return avif_size(bytes).ok_or_else(not_readable);
    };

    // Read the header only; the pixels are decoded later if at all
    let mut reader = ImageReader::with_format(Cursor::new(bytes), decoder_format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(|_| not_readable())?;
    let (width, height) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    Ok(if turns_a_quarter(orientation) { (height, width) } else { (width, height) })
}

/// Whether an EXIF orientation swaps width and height.
fn turns_a_quarter(orientation: Orientation) -> bool {
    matches!(orientation, Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH)
}

/// The largest `ispe` (image spatial extent) box in an AVIF file: the primary image's size.
/// Thumbnails and tiles are smaller, so the largest one is the picture.
fn avif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let read = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    bytes
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"ispe")
        .filter_map(|(i, _)| Some((read(i + 8)?, read(i + 12)?)))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .max_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
}

/// Whether Cilica may shrink this image: still images it can decode. GIFs and animated PNG or
/// WebP keep every frame by staying as they are.
fn can_shrink(bytes: &[u8], format: StoredFormat) -> bool {
    match format {
        StoredFormat::Jpeg | StoredFormat::Bmp => true,
        StoredFormat::Png => !is_animated_png(bytes),
        StoredFormat::Webp => !is_animated_webp(bytes),
        StoredFormat::Gif | StoredFormat::Avif => false,
    }
}

/// An APNG: an `acTL` chunk before the first `IDAT`.
fn is_animated_png(bytes: &[u8]) -> bool {
    let mut at = 8;
    while let Some(header) = bytes.get(at..at + 8) {
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        match &header[4..8] {
            b"acTL" => return true,
            b"IDAT" => return false,
            _ => at = at.saturating_add(12).saturating_add(len),
        }
    }
    false
}

/// A WebP whose extended header (`VP8X`) sets the animation flag.
fn is_animated_webp(bytes: &[u8]) -> bool {
    bytes.get(12..16) == Some(b"VP8X") && bytes.get(20).is_some_and(|flags| flags & 0x02 != 0)
}

/// Decode with limits and apply the EXIF orientation, so re-encoded pixels face the right way.
fn decode(bytes: &[u8], format: image::ImageFormat) -> AppResult<DynamicImage> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(|_| not_readable())?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| not_readable())?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// TIFF in, PNG out (the webview can't show TIFF).
fn tiff_to_png(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let image = decode(bytes, image::ImageFormat::Tiff)?;
    check_size(image.width(), image.height())?;
    encode_png(&image)
}

// ---------- For models ---------- //

/// Longest side of an image sent to a model. Vision models downscale anything larger, so bigger
/// only costs time and tokens.
pub const MODEL_MAX_SIDE: u32 = 1568;

/// An image ready to send to a model: re-encoded (so no metadata survives) and fitted inside
/// [`MODEL_MAX_SIDE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelImage {
    /// `image/jpeg`, or `image/png` when any pixel is see-through.
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl ModelImage {
    /// The bytes as standard, padded base64 (what model APIs take).
    pub fn to_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }
}

/// Turn stored bytes into what a model sees: decoded with limits, turned upright, fitted inside
/// `max_side` (never enlarged), re-encoded as JPEG or as PNG when it has transparency. Animated
/// images give their first frame. AVIF can't be decoded here and is refused.
pub fn for_model(bytes: &[u8], max_side: u32) -> AppResult<ModelImage> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(AppError::invalid("That image is larger than 40 MB."));
    }
    let format = match Sniffed::of(bytes) {
        Sniffed::Stored(format) => format,
        _ => return Err(not_readable()),
    };
    let decoder_format = format
        .decoder_format()
        .ok_or_else(|| AppError::invalid("Cilica can't show AVIF images to the model. Save it as PNG or JPEG to let Cilica see it."))?;
    let mut image = decode(bytes, decoder_format)?;
    check_size(image.width(), image.height())?;
    if image.width().max(image.height()) > max_side {
        image = image.resize(max_side, max_side, FilterType::Lanczos3);
    }
    let (width, height) = (image.width(), image.height());
    Ok(if has_transparency(&image) {
        ModelImage { media_type: "image/png", bytes: encode_png(&image)?, width, height }
    } else {
        ModelImage { media_type: "image/jpeg", bytes: encode_jpeg(&image)?, width, height }
    })
}

/// Fit the image inside [`SHRINK_TO`] (Lanczos3) and re-encode: JPEG when every pixel is opaque,
/// PNG when any is see-through.
fn shrink(bytes: &[u8], format: StoredFormat) -> AppResult<Prepared> {
    let decoder_format = format.decoder_format().ok_or_else(not_readable)?;
    let image = decode(bytes, decoder_format)?.resize(SHRINK_TO, SHRINK_TO, FilterType::Lanczos3);
    let (width, height) = (image.width(), image.height());
    if has_transparency(&image) {
        Ok(Prepared { bytes: encode_png(&image)?, format: StoredFormat::Png, width, height })
    } else {
        Ok(Prepared { bytes: encode_jpeg(&image)?, format: StoredFormat::Jpeg, width, height })
    }
}

/// Whether any pixel is less than fully opaque.
fn has_transparency(image: &DynamicImage) -> bool {
    image.color().has_alpha() && image.to_rgba8().pixels().any(|p| p.0[3] < u8::MAX)
}

/// Encode as PNG.
fn encode_png(image: &DynamicImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    image.write_with_encoder(PngEncoder::new(&mut out)).map_err(|e| AppError::internal(format!("Cilica couldn't save that image as PNG: {e}")))?;
    Ok(out)
}

/// Encode as JPEG at [`JPEG_QUALITY`] (JPEG has no alpha, so pixels go to RGB first).
fn encode_jpeg(image: &DynamicImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    let rgb = DynamicImage::ImageRgb8(image.to_rgb8());
    rgb.write_with_encoder(JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY))
        .map_err(|e| AppError::internal(format!("Cilica couldn't save that image as JPEG: {e}")))?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

    use super::*;

    /// An RGB image of the given size and format, as bytes.
    pub(crate) fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([200, 100, 50])));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    /// The preparer with no HEIC converter.
    fn plain(shrink_large: bool) -> Preparer<'static> {
        Preparer { shrink_large, heic: None }
    }

    #[test]
    fn keeps_common_formats_as_they_are() {
        for (format, stored) in [
            (ImageFormat::Png, StoredFormat::Png),
            (ImageFormat::Jpeg, StoredFormat::Jpeg),
            (ImageFormat::Gif, StoredFormat::Gif),
            (ImageFormat::Bmp, StoredFormat::Bmp),
            (ImageFormat::WebP, StoredFormat::Webp),
        ] {
            let bytes = encoded(7, 5, format);
            let prepared = plain(false).prepare(bytes.clone()).unwrap();
            assert_eq!(prepared.format, stored);
            assert_eq!((prepared.width, prepared.height), (7, 5));
            assert_eq!(prepared.bytes, bytes, "{format:?} should be stored untouched");
        }
    }

    #[test]
    fn images_for_models_are_fitted_and_re_encoded() {
        // A big opaque PNG becomes a JPEG inside the limit, keeping its shape
        let big = for_model(&encoded(3000, 1500, ImageFormat::Png), MODEL_MAX_SIDE).unwrap();
        assert_eq!(big.media_type, "image/jpeg");
        assert_eq!((big.width, big.height), (1568, 784));
        assert!(big.bytes.starts_with(&[0xFF, 0xD8]));

        // Small images are never enlarged; transparency keeps PNG
        let mut rgba = RgbaImage::from_pixel(10, 8, Rgba([0, 0, 0, 255]));
        rgba.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(rgba).write_to(&mut png, ImageFormat::Png).unwrap();
        let small = for_model(png.get_ref(), MODEL_MAX_SIDE).unwrap();
        assert_eq!((small.media_type, small.width, small.height), ("image/png", 10, 8));

        assert!(for_model(b"not an image", MODEL_MAX_SIDE).is_err());
    }

    #[test]
    fn turns_tiff_into_png() {
        let prepared = plain(false).prepare(encoded(6, 4, ImageFormat::Tiff)).unwrap();
        assert_eq!(prepared.format, StoredFormat::Png);
        assert_eq!((prepared.width, prepared.height), (6, 4));
        assert!(prepared.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn ignores_what_the_name_claims_and_refuses_svg_and_junk() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.to_vec();
        assert!(plain(false).prepare(svg).unwrap_err().message.contains("SVG"));
        let html = b"<html><body>hi</body></html>".to_vec();
        assert!(plain(false).prepare(html).unwrap_err().message.contains("isn't an image"));
        assert!(plain(false).prepare(Vec::new()).is_err());
    }

    #[test]
    fn refuses_a_truncated_image() {
        // A PNG signature with nothing after it
        let bytes = encoded(4, 4, ImageFormat::Png)[..12].to_vec();
        assert_eq!(plain(false).prepare(bytes).unwrap_err().message, "This file isn't an image Cilica can read.");
    }

    #[test]
    fn heic_needs_a_converter() {
        let mut heic = vec![0, 0, 0, 24];
        heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
        heic.extend_from_slice(&[0; 32]);
        let error = plain(false).prepare(heic.clone()).unwrap_err();
        assert!(error.message.starts_with("Cilica can't open HEIC images on this computer."));

        // With a converter, its JPEG is what gets stored
        let converter: HeicConverter = Arc::new(|_| Ok(encoded(3, 2, ImageFormat::Jpeg)));
        let prepared = Preparer { shrink_large: false, heic: Some(&converter) }.prepare(heic).unwrap();
        assert_eq!(prepared.format, StoredFormat::Jpeg);
        assert_eq!((prepared.width, prepared.height), (3, 2));
    }

    #[test]
    fn refuses_images_past_the_limits() {
        // A PNG header claiming 30000 × 10 pixels, refused before any pixels are read
        let mut png = encoded(1, 1, ImageFormat::Png);
        png[16..20].copy_from_slice(&30_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&10u32.to_be_bytes());
        assert!(plain(false).prepare(png).is_err());
        assert!(check_size(20_000, 5_001).is_err());
        assert!(check_size(20_000, 5_000).is_ok());
        assert!(plain(false).prepare(vec![0; MAX_INPUT_BYTES + 1]).unwrap_err().message.contains("40 MB"));
    }

    #[test]
    fn shrinks_big_photos_only_when_asked() {
        let big = encoded(3000, 1500, ImageFormat::Png);
        let kept = plain(false).prepare(big.clone()).unwrap();
        assert_eq!((kept.width, kept.height), (3000, 1500));

        // Opaque, so it becomes a JPEG that fits 2560
        let shrunk = plain(true).prepare(big).unwrap();
        assert_eq!(shrunk.format, StoredFormat::Jpeg);
        assert_eq!((shrunk.width, shrunk.height), (2560, 1280));
    }

    #[test]
    fn shrunk_see_through_images_stay_png() {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2600, 100, Rgba([0, 0, 0, 10])));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        let shrunk = plain(true).prepare(out.into_inner()).unwrap();
        assert_eq!(shrunk.format, StoredFormat::Png);
        assert_eq!(shrunk.width, 2560);
    }

    #[test]
    fn never_shrinks_gifs() {
        let gif = encoded(2600, 10, ImageFormat::Gif);
        let prepared = plain(true).prepare(gif.clone()).unwrap();
        assert_eq!(prepared.format, StoredFormat::Gif);
        assert_eq!(prepared.bytes, gif);
    }

    #[test]
    fn reads_the_size_of_an_avif() {
        // ftyp avif, then an ispe box for 640 × 480 and a smaller thumbnail one
        let mut avif = vec![0, 0, 0, 20];
        avif.extend_from_slice(b"ftypavif\0\0\0\0avif");
        for (w, h) in [(64u32, 48u32), (640, 480)] {
            avif.extend_from_slice(&20u32.to_be_bytes());
            avif.extend_from_slice(b"ispe\0\0\0\0");
            avif.extend_from_slice(&w.to_be_bytes());
            avif.extend_from_slice(&h.to_be_bytes());
        }
        let prepared = plain(true).prepare(avif).unwrap();
        assert_eq!(prepared.format, StoredFormat::Avif);
        assert_eq!((prepared.width, prepared.height), (640, 480));
    }
}
