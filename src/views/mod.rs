//! Two kinds of view live here. `auth` holds the JSON response shapes of
//! the `/api/auth` controller (Loco's convention). `layout` and the page
//! modules hold the Leptos components the page controllers render into HTML.

pub mod auth;
pub mod error;
pub mod home;
pub mod layout;
pub mod mail;
pub mod not_found;
pub mod reset;
pub mod too_many_requests;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    /// Every form a page renders names its `hx-sync`, as `ResetForm` does
    /// (`this:drop`). The shell's region sends every form through htmx, and
    /// htmx 4's default queues a second submit, which Enter in a field
    /// sends whatever the button shows: a second post, with a reset token
    /// the first one used up. A search as you type names `replace` instead.
    #[test]
    fn every_form_names_its_hx_sync() {
        let views = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views");
        let mut forms = 0;
        for entry in fs::read_dir(&views).expect("src/views") {
            let path = entry.expect("a file in src/views").path();
            // Not this file: it renders no page, and this test names `<form`.
            if path.extension().is_none_or(|extension| extension != "rs")
                || path.file_name().is_some_and(|name| name == "mod.rs")
            {
                continue;
            }
            let source = fs::read_to_string(&path).expect("a view");
            let code: Vec<&str> = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect();
            let code = code.join("\n");
            for (at, _) in code.match_indices("<form") {
                let tag = &code[at..code[at..].find('>').map_or(code.len(), |end| at + end)];
                assert!(
                    tag.contains("hx-sync="),
                    "{}: a <form> without hx-sync:\n{tag}",
                    path.display()
                );
                forms += 1;
            }
        }
        assert!(forms > 0, "no <form> found: has ResetForm moved?");
    }
}
