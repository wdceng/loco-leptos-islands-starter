//! Rule no. 1, one job, one implementation (CLAUDE.md, docs/one-job.md),
//! read as source where a one-job break could come back: each test names
//! a pattern that belongs to one job's one implementation, and the files it
//! may stand in, its home. A code line anywhere else under `src/` that
//! holds it fails, with the file and line and the row to build from.
//! Comments are not read: a comment may name what the code may not do.
//!
//! A new guard is one `assert_one_home` call, its doc saying which break it
//! keeps out and when it was found.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The project's `src/`.
fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `dir`, in a stable order.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    let mut found = Vec::new();
    for path in entries {
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}

/// `text`'s code lines, numbered from 1: a line that is a comment is not
/// code.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line))
}

/// Each `.rs` file under `src/` with its path from there and its text.
fn files() -> Vec<(String, String)> {
    let src = src();
    sources(&src)
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let text =
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (relative, text)
        })
        .collect()
}

/// The code lines under `src/` holding `pattern`, as `src/path:line`,
/// outside the files of `home` (paths from `src/`).
fn outside_home(pattern: &str, home: &[&str]) -> Vec<String> {
    files()
        .into_iter()
        .filter(|(relative, _)| !home.contains(&relative.as_str()))
        .flat_map(|(relative, text)| {
            code_lines(&text)
                .filter(|(_, line)| line.contains(pattern))
                .map(|(number, line)| format!("src/{relative}:{number}: {}", line.trim()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Fails with every line holding `pattern` outside `home`, and the row to
/// build from instead.
fn assert_one_home(pattern: &str, home: &[&str], row: &str) {
    let found = outside_home(pattern, home);
    assert!(
        found.is_empty(),
        "`{pattern}` outside its one implementation; build it from {row} (docs/one-job.md):\n{}",
        found.join("\n")
    );
}

/// A page's title is its own name, the shell adding the app's after it:
/// three pages wrote it themselves, two ways and once not at all, until
/// 2026-10-21.
#[test]
fn every_title_gets_the_apps_name_from_the_shell() {
    for pattern in ["| {APP_NAME}", "· {APP_NAME}", "- {APP_NAME}"] {
        assert_one_home(
            pattern,
            &["views/layout.rs"],
            "`full_title` (views/layout.rs), the title the page's name alone",
        );
    }
}

/// A page's heading and a page that only tells something: four pages wrote
/// the heading's classes, the line under it and the way home themselves
/// until 2026-10-21.
#[test]
fn every_heading_is_the_title_and_every_notice_the_notice() {
    for pattern in ["text-4xl", "Back to the home page"] {
        assert_one_home(
            pattern,
            &["views/layout.rs"],
            "`TITLE` and `LEAD`, or `Notice` (views/layout.rs)",
        );
    }
}

/// A form while it is sent is `aria-busy`, its button's look the `busy:`
/// variant, never `disabled`, which drops the button's name and value from
/// the post. An islands app's plain forms went `disabled` beside the busy
/// ones until 2026-10-21.
#[test]
fn every_form_is_sent_in_the_one_sending_state() {
    assert_one_home(
        "x-bind:disabled=\"sending\"",
        &[],
        "the form's `aria-busy` and the `busy:` variant, as `ResetForm` (views/reset.rs)",
    );
}

/// A panel closes on a tap outside at the tap's end, never at its start,
/// which also begins every scroll on a touch screen: an islands app's menu
/// closed at `pointerdown`, its search at `pointerup`, until 2026-10-21.
#[test]
fn every_tap_outside_closes_at_its_end() {
    assert_one_home(
        "pointerdown.outside",
        &[],
        "a close at `pointerup` (docs/development.md, Gotchas)",
    );
}

/// Every form names its `hx-sync`, as `ResetForm` does (`this:drop`). The
/// shell's region sends every form through htmx, and htmx 4's default
/// queues a second submit, which Enter in a field sends whatever the button
/// shows: a second post, with a reset token the first one used up. A
/// search as you type names `replace` instead.
#[test]
fn every_form_names_its_hx_sync() {
    let mut forms = 0;
    for (relative, text) in files() {
        let code: Vec<&str> = code_lines(&text).map(|(_, line)| line).collect();
        let code = code.join("\n");
        for (at, _) in code.match_indices("<form") {
            let tag = &code[at..code[at..].find('>').map_or(code.len(), |end| at + end)];
            assert!(
                tag.contains("hx-sync="),
                "src/{relative}: a <form> without hx-sync:\n{tag}"
            );
            forms += 1;
        }
    }
    assert!(forms > 0, "no <form> found: has ResetForm moved?");
}

/// The check itself: a code line holding the pattern is found, a comment
/// is not, and the home is left out.
#[test]
fn the_check_finds_code_and_leaves_comments_and_the_home() {
    let text = "// a comment naming `x`\nlet x = 1;\n  /// a doc naming `x`";
    let found: Vec<(usize, &str)> = code_lines(text)
        .filter(|(_, line)| line.contains('x'))
        .collect();
    assert_eq!(found, vec![(2, "let x = 1;")]);
    assert!(
        !outside_home("pub const TITLE", &[]).is_empty(),
        "the constant's own line not found"
    );
    assert!(
        outside_home("pub const TITLE", &["views/layout.rs"]).is_empty(),
        "its home not left out"
    );
}
