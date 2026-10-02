//! Versions every file in `public/`, the way cargo-leptos versions its own.
//!
//! cargo-leptos puts a content hash in the names of the bundle and the
//! stylesheet, so production can cache them for a year. The files in
//! `public/` keep their names, so a changed icon or font would stay cached
//! for a year too. This script hashes each file's bytes (FNV-1a, 64 bits,
//! no new crate) and writes one Rust constant per file into
//! `$OUT_DIR/public_assets.rs`, in modules that follow the folders:
//!
//! ```text
//! pub const FAVICON_ICO: &str = "/favicon.ico?v=<hash>";
//! pub mod fonts {
//!     pub const INTER_REGULAR_WOFF2: &str = "/fonts/Inter-Regular.woff2?v=<hash>";
//! }
//! ```
//!
//! `src/paths.rs` includes it as `paths::assets`. Rust code links a file
//! only through these constants, so a file that is renamed or removed fails
//! the compile. The stylesheet is a static file that can't read them; it
//! carries the same `?v=<hash>` written out, and a test in `src/paths.rs`
//! fails with the value to paste when a font changes.
//!
//! Cargo reruns this when anything in `public/` changes, is added or is
//! removed, new folders included, and the binary and `site/` of a deploy
//! come from the same checkout, so the hashes in the binary always match
//! the files it serves.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

const PUBLIC: &str = "public";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
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

/// `Inter-Regular.woff2` -> `INTER_REGULAR_WOFF2`; `fav-icons` ->
/// `fav_icons` for a module. A leading digit gets an `N`/`n` in front.
fn ident(name: &str, upper: bool) -> String {
    let ident: String = name
        .chars()
        .map(|c| match (c.is_ascii_alphanumeric(), upper) {
            (true, true) => c.to_ascii_uppercase(),
            (true, false) => c.to_ascii_lowercase(),
            (false, _) => '_',
        })
        .collect();
    match (ident.starts_with(|c: char| c.is_ascii_digit()), upper) {
        (true, true) => format!("N{ident}"),
        (true, false) => format!("n{ident}"),
        (false, _) => ident,
    }
}

/// One constant: its name, and the versioned URL.
type Entry = (String, String);

/// Writes the constants of `folder` and, nested, of every folder below it.
fn write_module(
    out: &mut String,
    tree: &BTreeMap<Vec<String>, Vec<Entry>>,
    folder: &[String],
    depth: usize,
) {
    let indent = "    ".repeat(depth);
    if let Some(entries) = tree.get(folder) {
        for (name, url) in entries {
            let _ = writeln!(out, "{indent}pub const {name}: &str = {url:?};");
        }
    }
    let children: Vec<&Vec<String>> = tree
        .keys()
        .filter(|key| key.len() == folder.len() + 1 && key.starts_with(folder))
        .collect();
    for child in children {
        let module = ident(child.last().expect("a folder name"), false);
        let _ = writeln!(out, "{indent}pub mod {module} {{");
        write_module(out, tree, child, depth + 1);
        let _ = writeln!(out, "{indent}}}");
    }
}

fn main() {
    println!("cargo:rerun-if-changed={PUBLIC}");
    let mut all = Vec::new();
    files(Path::new(PUBLIC), &mut all);
    // Sorted, so the generated file doesn't depend on the order the disk
    // lists files.
    all.sort();

    // Folder path -> its files' constants. Every folder on the way to a file
    // gets an entry, so a folder holding only folders still becomes a module.
    let mut tree: BTreeMap<Vec<String>, Vec<Entry>> = BTreeMap::new();
    let mut table = String::from("pub const ALL: &[(&str, &str)] = &[\n");
    for path in &all {
        let relative = path
            .strip_prefix(PUBLIC)
            .expect("under public/")
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(path)
            .unwrap_or_else(|e| panic!("build.rs: cannot read {}: {e}", path.display()));
        let url = format!("/{relative}?v={:016x}", fnv1a(&bytes));

        let mut parts: Vec<String> = relative.split('/').map(str::to_owned).collect();
        let file = parts.pop().expect("a file name");
        for depth in 0..=parts.len() {
            tree.entry(parts[..depth].to_vec()).or_default();
        }
        let name = ident(&file, true);
        let module_path: String = parts
            .iter()
            .map(|folder| format!("{}::", ident(folder, false)))
            .collect();
        let _ = writeln!(
            table,
            "    ({:?}, {module_path}{name}),",
            format!("/{relative}")
        );
        tree.entry(parts).or_default().push((name, url));
    }
    table.push_str("];\n");

    let mut out = String::from("// Generated by build.rs from public/. Don't edit.\n");
    write_module(&mut out, &tree, &[], 0);
    out.push_str(&table);

    let target = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("public_assets.rs");
    fs::write(&target, out)
        .unwrap_or_else(|e| panic!("build.rs: cannot write {}: {e}", target.display()));
}
