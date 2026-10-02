//! Addresses of the static files that cargo-leptos doesn't hash, each with
//! `?v=<hash>` appended. The hash is `ASSET_VERSION`, computed by `build.rs`
//! from the files in `public/favicon/` at compile time: replace an icon,
//! build, and every page links a new URL, so a browser fetches it once
//! despite the year-long cache. Nothing to rename by hand.
//!
//! Not covered: the two Android icons named inside `site.webmanifest` (a
//! static JSON file can't read the hash; it only matters when someone adds
//! the site to a home screen again), and the fonts, whose URLs live in the
//! stylesheet (`style/tailwind.css`): rename a font file when it changes.

/// The version of the files in `public/favicon/`: 16 hex characters.
pub const ASSET_VERSION: &str = env!("ASSET_VERSION");

/// `path` with `?v=<ASSET_VERSION>` appended, as a `&'static str`.
macro_rules! versioned {
    ($path:literal) => {
        concat!($path, "?v=", env!("ASSET_VERSION"))
    };
}

pub const FAVICON: &str = versioned!("/favicon/favicon.ico");
pub const FAVICON_32: &str = versioned!("/favicon/favicon-32x32.png");
pub const FAVICON_16: &str = versioned!("/favicon/favicon-16x16.png");
pub const APPLE_TOUCH_ICON: &str = versioned!("/favicon/apple-touch-icon.png");
pub const MANIFEST: &str = versioned!("/favicon/site.webmanifest");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_16_hex_characters() {
        assert_eq!(ASSET_VERSION.len(), 16, "{ASSET_VERSION}");
        assert!(
            ASSET_VERSION.chars().all(|c| c.is_ascii_hexdigit()),
            "{ASSET_VERSION}"
        );
    }

    #[test]
    fn every_versioned_path_ends_with_the_version() {
        for path in [FAVICON, FAVICON_32, FAVICON_16, APPLE_TOUCH_ICON, MANIFEST] {
            assert!(path.starts_with("/favicon/"), "{path}");
            assert!(path.ends_with(&format!("?v={ASSET_VERSION}")), "{path}");
        }
    }
}
