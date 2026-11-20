//! Optional JPEG/PNG → WebP conversion when the site enables image optimization
//! and the client advertises `Accept: image/webp`.

use http::{header, HeaderMap, StatusCode};

/// Hard cap so a huge photo cannot pin a worker converting forever.
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

/// Concurrent conversions (each holds up to `MAX_INPUT_BYTES` in memory).
const MAX_CONCURRENT_CONVERSIONS: usize = 4;

/// Whether this build can encode WebP at all. Without the `image-webp`
/// feature the site toggle is accepted but nothing is converted.
pub fn conversion_available() -> bool {
    cfg!(feature = "image-webp")
}

/// Reserve a conversion slot; `None` when all are busy (serve the original only).
pub fn try_conversion_slot() -> Option<tokio::sync::OwnedSemaphorePermit> {
    static SLOTS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    SLOTS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONVERSIONS)))
        .clone()
        .try_acquire_owned()
        .ok()
}

/// Whether the client will accept a WebP response. Only an explicit
/// `image/webp` counts: older Safari sends `image/*` and `*/*` for images but
/// cannot decode WebP, and curl, HTTP libraries and mail clients send `*/*`
/// while expecting the JPEG/PNG bytes the URL names.
pub fn accepts_webp(accept: &str) -> bool {
    accept
        .split(',')
        .map(|part| {
            let part = part.trim();
            let coding = part.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
            let q = part
                .split(';')
                .skip(1)
                .find_map(|p| {
                    let p = p.trim();
                    p.strip_prefix("q=")
                        .and_then(|v| v.trim().parse::<f32>().ok())
                })
                .unwrap_or(1.0);
            (coding, q)
        })
        .any(|(coding, q)| q > 0.0 && coding == "image/webp")
}

fn is_raster_source(content_type: &str) -> bool {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(ct.as_str(), "image/jpeg" | "image/jpg" | "image/png")
}

/// Decide whether this response is a WebP conversion candidate.
pub fn should_optimize_webp(
    enabled: bool,
    status: StatusCode,
    response: &HeaderMap,
    request: &HeaderMap,
    body_len: usize,
) -> bool {
    if !enabled || status != StatusCode::OK || body_len == 0 || body_len > MAX_INPUT_BYTES {
        return false;
    }
    if response.contains_key(header::CONTENT_RANGE) {
        return false;
    }
    if response
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            let v = v.trim();
            !v.is_empty() && !v.eq_ignore_ascii_case("identity")
        })
    {
        return false;
    }
    let content_type = response
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !is_raster_source(content_type) {
        return false;
    }
    let accept = request
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    accepts_webp(accept)
}

/// Convert JPEG/PNG bytes to WebP. Returns `None` on failure (caller keeps original).
pub fn to_webp(input: &[u8], quality: u8) -> Option<Vec<u8>> {
    #[cfg(feature = "image-webp")]
    {
        let img = image::load_from_memory(input).ok()?;
        let rgba = img.to_rgba8();
        let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
        let encoded = encoder.encode(quality.clamp(1, 100) as f32);
        Some(encoded.to_vec())
    }
    #[cfg(not(feature = "image-webp"))]
    {
        let _ = (input, quality);
        tracing::debug!("image-webp feature disabled; skipping conversion");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_webp_parsing() {
        assert!(accepts_webp("image/webp"));
        assert!(accepts_webp("image/avif,image/webp,image/apng,*/*;q=0.8"));
        assert!(!accepts_webp("image/png"));
        assert!(!accepts_webp("image/webp;q=0"));
        assert!(accepts_webp("IMAGE/WEBP;q=0.9"));
        // Wildcards alone do not mean the client can decode WebP.
        assert!(!accepts_webp("*/*"));
        assert!(!accepts_webp("image/*"));
        assert!(!accepts_webp("image/png,image/*;q=0.8,*/*;q=0.5"));
        assert!(!accepts_webp(""));
    }

    #[test]
    fn non_image_input_is_not_converted() {
        assert!(to_webp(b"definitely not an image", 80).is_none());
        assert!(to_webp(&[], 80).is_none());
    }

    /// The release binary must actually encode: this fails if libwebp is
    /// missing or broken in the build, not just when the feature is off.
    #[cfg(feature = "image-webp")]
    #[test]
    fn converts_jpeg_and_png_to_webp() {
        assert!(conversion_available());
        let pixels = image::RgbaImage::from_fn(32, 24, |x, y| {
            image::Rgba([(x * 8) as u8, (y * 10) as u8, 128, 255])
        });
        for format in [image::ImageFormat::Jpeg, image::ImageFormat::Png] {
            let mut input = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(pixels.clone())
                .to_rgb8()
                .write_to(&mut input, format)
                .expect("encode test image");
            let input = input.into_inner();

            let webp = to_webp(&input, 80).expect("conversion succeeds");
            assert_eq!(&webp[..4], b"RIFF", "{format:?}");
            assert_eq!(&webp[8..12], b"WEBP", "{format:?}");
            let decoded = webp::Decoder::new(&webp).decode().expect("valid WebP");
            assert_eq!((decoded.width(), decoded.height()), (32, 24), "{format:?}");
        }
    }
}
