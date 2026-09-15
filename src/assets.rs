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
/// deployed without its hash file. A missing site folder is not an error
/// here; Loco's `static` middleware decides whether that is allowed. The
/// test environment skips the on-disk checks: the harness serves no
/// assets, and `target/site` may hold whatever the last build left there.
pub fn detect(options: &mut LeptosOptions, env: &Environment) -> Result<Assets> {
    let hash_path = hash_file_path(options);
    let pkg = options.site_pkg_dir.as_ref();
    let name = options.output_name.as_ref();
    let pkg_dir = Path::new(options.site_root.as_ref()).join(pkg);
    let check_disk = !matches!(env, Environment::Test);

    let file = match std::fs::read_to_string(&hash_path) {
        Ok(contents) => {
            let hash = css_hash(&contents).ok_or_else(|| {
                Error::Message(format!("{}: no `css:` line", hash_path.display()))
            })?;
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
                    "{} has no {file} and no {} exists next to the binary: a hashed build must be deployed together with its hash file (see README.DEPLOY.md); locally, `cargo leptos build` restores plain names",
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

    Ok(Assets {
        stylesheet: format!("/{pkg}/{file}"),
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

/// The hash on the `css:` line of a cargo-leptos hash file.
fn css_hash(contents: &str) -> Option<&str> {
    contents
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(key, _)| key.trim() == "css")
        .map(|(_, hash)| hash.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_hash_is_read_from_the_hash_file() {
        let contents = "js: 1a2b3c\nwasm: 4d5e6f\ncss: 7a8b9c\n";
        assert_eq!(css_hash(contents), Some("7a8b9c"));
    }

    #[test]
    fn css_hash_is_none_without_a_css_line() {
        assert_eq!(css_hash("js: 1a2b3c\nwasm: 4d5e6f\n"), None);
        assert_eq!(css_hash(""), None);
    }
}
