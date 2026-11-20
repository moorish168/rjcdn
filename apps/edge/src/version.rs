//! Build version stamped by `build.rs` at compile time.

pub const GIT_HASH: &str = env!("LAGRAN_GIT_HASH");
pub const VERSION: &str = env!("LAGRAN_BUILD_VERSION");

/// Optional Cargo features compiled into this binary, as printed by
/// `lagran-edge version`. The release job checks it for `image-webp`, and the
/// console's remote upgrade log shows it, so a build that cannot serve the
/// 「图片自动 WebP」 switch is visible instead of silently serving originals.
pub const FEATURES: &str = if cfg!(feature = "image-webp") {
    "image-webp"
} else {
    "none"
};
