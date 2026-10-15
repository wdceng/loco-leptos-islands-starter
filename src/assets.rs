//! Hashed asset names for long-lived caching.
//!
//! Release builds run with `LEPTOS_HASH_FILES=true`, which makes cargo-leptos
//! rename its outputs to `<output-name>.<hash>.js/.wasm/.css` and write the
//! hashes to `hash.txt` next to the server binary. The deploy ships that
//! file with the binary. Leptos's `HydrationScripts` reads it to name the JS
//! and wasm files; this module does the same for the stylesheet, which the
//! shell links by hand, and decides at boot whether hashing is on at all:
//! the hash file next to the running binary is the single signal, so a dev
//! build (no file) serves plain names and a deployed build serves hashed
//! ones without any further configuration. Either way the chosen stylesheet
//! must exist in the site folder, so a hash file and a site folder from
//! different builds, or a hashed site without its hash file, fail the boot
//! instead of serving a page whose stylesheet is a 404.
//!
//! Hashing is release-only because the `cargo leptos watch` loop hashes
//! only on its first build; later incremental rebuilds would leave the hash
//! file stale.
//!
//! The same boot check reads the main script for the split build. With a
//! lazy island (`#[island(lazy)]`, `src/islands.rs`) every cargo-leptos
//! command needs `--split`: without it the script imports a module named
//! `__wasm_split_placeholder__`, which no browser can load, so no island on
//! any page runs. The boot refuses that build, and refuses a script that
//! names a split loader (`__wasm_split.<hash>.js`) the site folder lacks.

use std::path::{Path, PathBuf};

use leptos::config::LeptosOptions;
use loco_rs::{Error, Result, environment::Environment};

/// Resolved, request-independent asset paths.
#[derive(Clone, Debug)]
pub struct Assets {
    /// Absolute URL path of the stylesheet, hashed or plain.
    pub stylesheet: String,
}

/// Looks for the hash file next to the running binary and resolves the
/// stylesheet path. Turns `options.hash_files` on when the file is there,
/// so `HydrationScripts` names the bundle the same way.
///
/// # Errors
/// The hash file is unreadable or has no `css:` line; or the stylesheet it
/// implies (hashed with the file, plain without) is not in the site folder
/// even though that folder exists: a stale hash file, or a hashed build
/// deployed without its hash file. The main script was built without
/// `--split` while a lazy island exists, or names a split loader the site
/// folder lacks. A missing site folder is not an error
/// here; Loco's `static` middleware decides whether that is allowed. The
/// test environment skips the on-disk checks: the harness serves no
/// assets, and `target/site` may hold whatever the last build left there.
pub fn detect(options: &mut LeptosOptions, env: &Environment) -> Result<Assets> {
    let hash_path = hash_file_path(options);
    let pkg = options.site_pkg_dir.as_ref();
    let name = options.output_name.as_ref();
    let pkg_dir = Path::new(options.site_root.as_ref()).join(pkg);
    let check_disk = !matches!(env, Environment::Test);
    // The main script, hashed like the stylesheet or plain.
    let mut script = format!("{name}.js");

    let file = match std::fs::read_to_string(&hash_path) {
        Ok(contents) => {
            let hash = hash_line(&contents, "css").ok_or_else(|| {
                Error::Message(format!("{}: no `css:` line", hash_path.display()))
            })?;
            if let Some(js) = hash_line(&contents, "js") {
                script = format!("{name}.{js}.js");
            }
            let file = format!("{name}.{hash}.css");
            if check_disk && !pkg_dir.join(&file).exists() {
                return Err(Error::Message(format!(
                    "{} names {file} but {} does not contain it: the hash file is stale, rebuild with LEPTOS_HASH_FILES=true or delete it",
                    hash_path.display(),
                    pkg_dir.display()
                )));
            }
            options.hash_files = true;
            file
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let file = format!("{name}.css");
            if check_disk && pkg_dir.exists() && !pkg_dir.join(&file).exists() {
                return Err(Error::Message(format!(
                    "{} has no {file} and no {} exists next to the binary: a hashed build must be deployed together with its hash file (see docs/staging.md, docs/production.md); locally, `cargo leptos build --split` restores plain names",
                    pkg_dir.display(),
                    hash_path.display()
                )));
            }
            file
        }
        Err(e) => {
            return Err(Error::Message(format!(
                "reading {}: {e}",
                hash_path.display()
            )));
        }
    };

    if check_disk && let Ok(text) = std::fs::read_to_string(pkg_dir.join(&script)) {
        split_problem(&text, |loader| pkg_dir.join(loader).exists()).map_or(Ok(()), |problem| {
            Err(Error::Message(format!(
                "{}: {problem}",
                pkg_dir.join(&script).display()
            )))
        })?;
    }

    Ok(Assets {
        stylesheet: format!("/{pkg}/{file}"),
    })
}

/// What the main script imports in place of the split loader when it was
/// built without `--split`.
const SPLIT_PLACEHOLDER: &str = "__wasm_split_placeholder__";

/// Why a main script's split build can't run, or `None` when it can: built
/// without `--split` while a lazy island exists, or naming a split loader
/// for which `exists` says no. A script with no lazy island names neither.
fn split_problem(script: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    if script.contains(SPLIT_PLACEHOLDER) {
        return Some(
            "built without --split while a lazy island exists, so no island would run: rebuild with `--split` on the cargo-leptos command".into(),
        );
    }
    split_loaders(script)
        .find(|loader| !exists(loader))
        .map(|loader| format!("names the split loader {loader}, which the site folder lacks: rebuild, or deploy the whole site folder of one build"))
}

/// The split loaders a main script imports, `__wasm_split.<hash>.js`, the
/// hash plain underscores in an unhashed build.
fn split_loaders(script: &str) -> impl Iterator<Item = &str> {
    const PREFIX: &str = "__wasm_split.";
    script.match_indices(PREFIX).filter_map(|(start, _)| {
        let rest = &script[start + PREFIX.len()..];
        let hash = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(rest.len());
        rest[hash..]
            .starts_with(".js")
            .then(|| &script[start..start + PREFIX.len() + hash + 3])
    })
}

/// Same lookup `HydrationScripts` performs: the hash file lives next to the
/// executable, not in the site folder.
fn hash_file_path(options: &LeptosOptions) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(options.hash_file.as_ref())
}

/// The hash on a cargo-leptos hash file's line for `kind` (`css`, `js`).
fn hash_line<'a>(contents: &'a str, kind: &str) -> Option<&'a str> {
    contents
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(key, _)| key.trim() == kind)
        .map(|(_, hash)| hash.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNHASHED_LOADER: &str = "__wasm_split.______________________.js";

    #[test]
    fn hashes_are_read_from_the_hash_file() {
        let contents = "js: 1a2b3c\nwasm: 4d5e6f\ncss: 7a8b9c\nsplit: 0d0e0f\n";
        assert_eq!(hash_line(contents, "css"), Some("7a8b9c"));
        assert_eq!(hash_line(contents, "js"), Some("1a2b3c"));
    }

    #[test]
    fn a_missing_line_is_none() {
        assert_eq!(hash_line("js: 1a2b3c\nwasm: 4d5e6f\n", "css"), None);
        assert_eq!(hash_line("", "css"), None);
    }

    /// As cargo-leptos 0.3 writes them: the import at the top, the name
    /// again in the import object.
    fn script_naming(loader: &str) -> String {
        format!("import * as import1 from \"./{loader}\"\n\"./{loader}\": import1,\n")
    }

    #[test]
    fn a_build_without_split_is_refused() {
        let script = "import * as import1 from \"__wasm_split_placeholder__\"\n";
        let problem = split_problem(script, |_| true).expect("refused");
        assert!(problem.contains("--split"), "{problem}");
    }

    #[test]
    fn the_loaders_are_found_plain_and_hashed() {
        let plain = script_naming(UNHASHED_LOADER);
        assert_eq!(
            split_loaders(&plain).collect::<Vec<_>>(),
            [UNHASHED_LOADER, UNHASHED_LOADER]
        );
        let hashed = script_naming("__wasm_split.aB3-x_9Q.js");
        assert_eq!(
            split_loaders(&hashed).next(),
            Some("__wasm_split.aB3-x_9Q.js")
        );
    }

    #[test]
    fn a_missing_loader_is_refused_and_a_present_one_passes() {
        let script = script_naming(UNHASHED_LOADER);
        let problem = split_problem(&script, |_| false).expect("refused");
        assert!(problem.contains(UNHASHED_LOADER), "{problem}");
        assert_eq!(split_problem(&script, |name| name == UNHASHED_LOADER), None);
    }

    #[test]
    fn a_script_without_lazy_islands_passes() {
        assert_eq!(
            split_problem("export function hydrate() {}", |_| false),
            None
        );
    }
}
