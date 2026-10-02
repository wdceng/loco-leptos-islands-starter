//! Versions the static files cargo-leptos doesn't hash.
//!
//! cargo-leptos puts a content hash in the names of the bundle and the
//! stylesheet, so production can cache them for a year. The icons and the
//! web manifest in `public/favicon/` keep their names, so a changed icon
//! would stay cached for a year too. This script hashes that folder (every
//! file's path and bytes, FNV-1a, 64 bits) and hands the result to the
//! compiler as `ASSET_VERSION`. `src/paths.rs` appends it to each link as
//! `?v=<hash>`, so a changed file gets a new URL and browsers fetch it once.
//!
//! Cargo reruns this when anything in the folder changes, and the binary
//! and `site/` of a deploy come from the same checkout, so the hash in the
//! binary always matches the files it serves.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The folders whose files are linked with `?v=<hash>`.
const VERSIONED: &[&str] = &["public/favicon"];

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Every file under `dir`, recursively.
fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path.file_name().is_some_and(|name| name != ".DS_Store") {
            out.push(path);
        }
    }
}

fn main() {
    let mut all = Vec::new();
    for dir in VERSIONED {
        println!("cargo:rerun-if-changed={dir}");
        files(Path::new(dir), &mut all);
    }
    // Sorted, so the hash doesn't depend on the order the disk lists files.
    all.sort();

    let mut hash = FNV_OFFSET;
    for path in &all {
        hash = fnv1a(hash, path.to_string_lossy().as_bytes());
        hash = fnv1a(hash, &[0]);
        let bytes = fs::read(path)
            .unwrap_or_else(|e| panic!("build.rs: cannot read {}: {e}", path.display()));
        hash = fnv1a(hash, &bytes);
        hash = fnv1a(hash, &[0]);
    }
    println!("cargo:rustc-env=ASSET_VERSION={hash:016x}");
}
