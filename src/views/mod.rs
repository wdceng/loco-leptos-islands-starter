//! Two kinds of view live here. `auth` holds the JSON response shapes of
//! the `/api/auth` controller (Loco's convention). `layout` and the page
//! modules hold the Leptos components the page controllers render into HTML.

pub mod auth;
pub mod error;
pub mod home;
pub mod layout;
pub mod mail;
pub mod not_found;
pub mod too_many_requests;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use regex::Regex;

    /// Every `.rs` file under `dir`, recursively.
    fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// Corners are named by role (`shape-control`, `shape-small`,
    /// `shape-row`, `shape-panel` in style/tailwind.css), never a raw
    /// `rounded-*`: one place to change every button's corner, and the
    /// phone/desktop/squircle rules apply everywhere.
    #[test]
    fn markup_names_a_shape_role_never_a_raw_radius() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![root.join("src/islands.rs")];
        rust_files(&root.join("src/views"), &mut files);
        let class = Regex::new(r#"(?:^|[\s"':])(rounded(?:-[A-Za-z0-9\[\]./%-]+)?)(?:[\s"]|$)"#)
            .expect("regex");

        let mut found = Vec::new();
        for file in &files {
            let text = fs::read_to_string(file).expect("readable");
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for hit in class.captures_iter(line) {
                    found.push(format!(
                        "{}:{}: `{}`",
                        file.strip_prefix(root).unwrap_or(file).display(),
                        n + 1,
                        &hit[1]
                    ));
                }
            }
        }
        assert!(
            found.is_empty(),
            "use a shape-* role, not a raw radius:\n{}",
            found.join("\n")
        );
    }
}
