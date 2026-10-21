//! The browser tests' harness (docs/testing.md, "Browser tests").
//!
//! `e2e/run.sh` builds the site, starts it (`LOCO_ENV=e2e`,
//! config/e2e.yaml) and chromedriver, then runs these tests with where each
//! listens in the environment. A test opens a session with [`browse`],
//! which closes it whatever the test does, keeps what a failed test's page
//! showed, and fails a test whose page logged an error or a warning.
//!
//! One file a job: a test's session (here), the page and what any test does
//! with it (`page`), Chrome's DevTools (`devtools`).

use std::{future::Future, panic, time::Duration};

mod devtools;
mod page;

pub use devtools::{Answer, Held};
pub use page::Page;
pub use thirtyfour::prelude::*;

/// How long a wait waits: the debug build's WASM is several megabytes.
pub(crate) const WAIT: Duration = Duration::from_secs(30);

/// How often a wait asks again.
pub(crate) const POLL: Duration = Duration::from_millis(100);

/// How long a person's press holds the mouse's button down: long enough
/// for the press to preload its page before the click (`Page::press`).
pub(crate) const PRESS_HELD: Duration = Duration::from_millis(300);

/// How long one test may run; the memory tests take about half a minute.
const TEST_WITHIN: Duration = Duration::from_secs(180);

/// The reset page with a token: a page without the lazy island, with a
/// form, long enough to scroll in a short window.
pub const RESET: &str = "/reset?token=e2e";

/// The header's link home, on every page.
pub const HOME_LINK: &str = "header a[href='/']";

/// Where the run's parts listen, from `e2e/run.sh`.
struct Run {
    base: String,
    driver: String,
    dir: String,
    headed: bool,
    /// Another Chrome than the installed one, the low bar's
    /// (`e2e/run.sh low`, `E2E_BINARY`).
    binary: Option<String>,
}

impl Run {
    /// Read from the environment `e2e/run.sh` sets; a test run without it
    /// is refused with the way to start one.
    fn from_env() -> Self {
        let var = |name: &str| {
            std::env::var(name).unwrap_or_else(|_| {
                panic!("{name} is not set: run the browser tests with e2e/run.sh")
            })
        };
        Self {
            base: var("E2E_BASE"),
            driver: var("E2E_DRIVER"),
            dir: var("E2E_DIR"),
            headed: std::env::var_os("E2E_HEADED").is_some(),
            binary: std::env::var("E2E_BINARY").ok().filter(|b| !b.is_empty()),
        }
    }
}

/// Open a Chrome session, run `test` with its page, and close the session
/// whatever happened: a failure or a panic is passed on after the session
/// is gone. A test that passed still fails when its page logged an error,
/// a warning or a refused resource ([`Page::allow_failed_load`] names the
/// expected), and a failure of either kind keeps what the page showed with
/// the lines it logged.
pub async fn browse<F, Fut>(test: F)
where
    F: FnOnce(Page) -> Fut,
    Fut: Future<Output = WebDriverResult<()>> + Send + 'static,
{
    let run = Run::from_env();
    // The runner names a test's thread after the test.
    let name = std::thread::current().name().unwrap_or("a test").to_owned();
    let driver = match open(&run).await {
        Ok(driver) => driver,
        Err(e) => panic!("no Chrome session: {e}"),
    };
    let page = match Page::start(driver.clone(), run.base.clone()).await {
        Ok(page) => page,
        Err(e) => {
            let _ = driver.quit().await;
            panic!("no DevTools session: {e}");
        }
    };
    let running = tokio::spawn(test(page.clone()));
    let stop = running.abort_handle();
    let outcome = match tokio::time::timeout(TEST_WITHIN, running).await {
        Ok(joined) => joined,
        Err(_) => {
            stop.abort();
            Ok(Err(WebDriverError::Timeout(format!(
                "the test ran past {TEST_WITHIN:?}, stopped"
            ))))
        }
    };
    // What failed, said; a panic's payload kept to be passed on.
    let mut failed = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some((e.to_string(), None)),
        Err(join) => {
            let payload = join.into_panic();
            let said = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "a panic".to_owned());
            Some((said, Some(payload)))
        }
    };
    // The page's own errors fail the test too, and join a failure's words:
    // a console error is often why the test failed.
    let problems = page.problems();
    if !problems.is_empty() {
        let logged = format!("the page logged:\n{}", problems.join("\n"));
        failed = Some(match failed {
            None => (logged, None),
            Some((said, payload)) => (format!("{said}\n\n{logged}"), payload),
        });
    }
    if let Some((said, _)) = &failed {
        evidence(&driver, &run.dir, &name, said).await;
    }
    page.stop();
    if let Err(e) = driver.quit().await {
        eprintln!("closing the session: {e}");
    }
    match failed {
        None => {}
        // The panic's own message is in its payload; the rest said here.
        Some((said, Some(payload))) => {
            eprintln!("{said}");
            panic::resume_unwind(payload)
        }
        Some((said, None)) => panic!("{said}"),
    }
}

/// A Chrome session, headless unless `E2E_HEADED` is set, its window a
/// desktop's, in the low bar's Chrome when `E2E_BINARY` names one.
async fn open(run: &Run) -> WebDriverResult<WebDriver> {
    let mut caps = DesiredCapabilities::chrome();
    if let Some(binary) = &run.binary {
        caps.set_binary(binary)?;
    }
    if !run.headed {
        caps.add_arg("--headless=new")?;
    }
    caps.add_arg("--window-size=1280,900")?;
    WebDriver::new(run.driver.as_str(), caps).await
}

/// What the page showed when its test failed, kept in `dir`'s `failures/`
/// for a look instead of a rerun: a screenshot, the page's HTML, its
/// address with what failed, and the server log's last lines. Whatever
/// the driver won't give within a few seconds is left out.
async fn evidence(driver: &WebDriver, dir: &str, test: &str, said: &str) {
    let within = Duration::from_secs(10);
    let folder = format!("{dir}/failures");
    if std::fs::create_dir_all(&folder).is_err() {
        return;
    }
    let base = format!("{folder}/{}", test.replace("::", "."));
    let url = match tokio::time::timeout(within, driver.current_url()).await {
        Ok(Ok(url)) => url.to_string(),
        _ => "(no answer)".to_owned(),
    };
    let _ = std::fs::write(format!("{base}.txt"), format!("{url}\n\n{said}\n"));
    if let Ok(Ok(png)) = tokio::time::timeout(within, driver.screenshot_as_png()).await {
        let _ = std::fs::write(format!("{base}.png"), png);
    }
    if let Ok(Ok(html)) = tokio::time::timeout(within, driver.source()).await {
        let _ = std::fs::write(format!("{base}.html"), html);
    }
    if let Ok(log) = std::fs::read_to_string(format!("{dir}/server.log")) {
        let lines: Vec<&str> = log.lines().collect();
        let tail = lines[lines.len().saturating_sub(80)..].join("\n");
        let _ = std::fs::write(format!("{base}.server.log"), tail);
    }
    eprintln!("what the page showed: {base}.png, .html, .txt, .server.log");
}
