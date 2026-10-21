//! A browser session on the run's site, and what any test does with it:
//! open a page and wait for it, read it, wait for a state, press.
//!
//! No element is ever the driver's: chromedriver keeps each element it
//! hands a test in a cache inside the page, and one kept link of a page
//! moved away from holds that page's whole detached tree, which the memory
//! tests would read as a leak (the sister app found it, 2026-10-10). A
//! press is a mouse's, sent through DevTools at a point the page's script
//! measured.

use std::time::Duration;

use serde_json::{Value, json};
use thirtyfour::cdp::CdpSession;
use thirtyfour::prelude::*;
use tokio::task::AbortHandle;

use crate::{POLL, PRESS_HELD, WAIT, devtools::Shared};

/// A browser session on the run's site.
#[derive(Clone)]
pub struct Page {
    pub driver: WebDriver,
    pub(crate) cdp: CdpSession,
    pub(crate) base: String,
    pub(crate) heard: Shared,
    pub(crate) watcher: AbortHandle,
}

impl Page {
    /// The site's full address of `path`.
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Load `path` whole and wait until the page is [`Page::ready`].
    pub async fn open(&self, path: &str) -> WebDriverResult<()> {
        self.driver.goto(self.url(path)).await?;
        self.until(
            &format!("{path} loaded"),
            &format!(
                "return location.pathname + location.search === {path:?}
                     && document.readyState === 'complete'"
            ),
        )
        .await?;
        self.ready().await
    }

    /// Wait until htmx and Alpine have started and every island on the
    /// page is awake: the debug build's `data-awake` on each
    /// `<leptos-island>` (src/collect.rs, `mark_awake`), since Leptos's
    /// loader says nothing when it's done. `e2e/run.sh` builds a debug
    /// site; a release one carries no mark.
    pub async fn ready(&self) -> WebDriverResult<()> {
        self.until(
            "htmx, Alpine and the islands",
            "return !!(window.htmx && window.Alpine)
                 && [...document.querySelectorAll('leptos-island')].every(i => i.hasAttribute('data-awake'))",
        )
        .await
    }

    /// Wait until no request of htmx's is on its way (`htmx-request` on its
    /// element) and the page is ready again.
    pub async fn settled(&self) -> WebDriverResult<()> {
        self.until(
            "htmx's requests answered",
            "return !document.querySelector('.htmx-request')",
        )
        .await?;
        self.ready().await
    }

    /// Wait until `script` returns true, or fail naming `what`.
    pub async fn until(&self, what: &str, script: &str) -> WebDriverResult<()> {
        self.until_within(what, script, WAIT).await
    }

    /// [`Page::until`] with a time of its own.
    pub async fn until_within(
        &self,
        what: &str,
        script: &str,
        within: Duration,
    ) -> WebDriverResult<()> {
        let started = tokio::time::Instant::now();
        loop {
            if self.eval(script).await?.as_bool() == Some(true) {
                return Ok(());
            }
            if started.elapsed() > within {
                return Err(WebDriverError::Timeout(format!(
                    "{what}: not within {within:?}"
                )));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// What `script` returns.
    pub async fn eval(&self, script: &str) -> WebDriverResult<Value> {
        Ok(self
            .driver
            .execute(script, Vec::new())
            .await?
            .json()
            .clone())
    }

    /// What `script` returns, as text.
    pub async fn text(&self, script: &str) -> WebDriverResult<String> {
        Ok(self
            .eval(script)
            .await?
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    /// The page's path and query.
    pub async fn path(&self) -> WebDriverResult<String> {
        self.text("return location.pathname + location.search")
            .await
    }

    /// Whether the page's text holds `words`.
    pub async fn says(&self, words: &str) -> WebDriverResult<bool> {
        Ok(self
            .eval(&format!(
                "return document.body.innerText.includes({words:?})"
            ))
            .await?
            == Value::Bool(true))
    }

    /// Wait until nothing on the page moves: no animation or transition
    /// running, so a press never lands on what still slides away.
    pub async fn still(&self) -> WebDriverResult<()> {
        self.until(
            "the page still",
            "return document.getAnimations().every(a => a.playState !== 'running')",
        )
        .await
    }

    /// Press the first element `css` finds as a person's mouse does: the
    /// pointer onto it, the button down, held long enough for the press to
    /// preload its page (`PRELOAD`, src/views/layout.rs), then up, the click
    /// with it. With the answer there before the click, htmx can swap it in
    /// before Alpine's click runs, which an instant script click never
    /// shows (tests/requests/races.rs). The page still first.
    pub async fn press(&self, css: &str) -> WebDriverResult<()> {
        self.still().await?;
        let (x, y) = self.centre(css).await?;
        for kind in ["mouseMoved", "mousePressed"] {
            self.mouse(kind, x, y).await?;
        }
        tokio::time::sleep(PRESS_HELD).await;
        self.mouse("mouseReleased", x, y).await
    }

    /// The centre of the first element `css` finds, in the window's
    /// pixels, scrolled into view only where it isn't (`nearest`, so a
    /// test's own scroll stays); waited for.
    async fn centre(&self, css: &str) -> WebDriverResult<(f64, f64)> {
        let started = tokio::time::Instant::now();
        loop {
            let at = self
                .eval(&format!(
                    "const e = document.querySelector({css:?});
                     if (!e) return null;
                     e.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
                     const r = e.getBoundingClientRect();
                     return [r.left + r.width / 2, r.top + r.height / 2]"
                ))
                .await?;
            if let (Some(x), Some(y)) = (at[0].as_f64(), at[1].as_f64()) {
                return Ok((x, y));
            }
            if started.elapsed() > WAIT {
                return Err(WebDriverError::Timeout(format!(
                    "{css}: not on the page within {WAIT:?}"
                )));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// One mouse event at (`x`, `y`), the left button's, through DevTools.
    async fn mouse(&self, kind: &str, x: f64, y: f64) -> WebDriverResult<()> {
        self.devtools(
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1 }),
        )
        .await
        .map(drop)
    }

    /// A move in place by the link `css`, to `path`, waited for.
    pub async fn move_to(&self, css: &str, path: &str) -> WebDriverResult<()> {
        self.press(css).await?;
        self.until(
            &format!("{path} in place"),
            &format!("return location.pathname + location.search === {path:?}"),
        )
        .await?;
        self.settled().await
    }

    /// A link to `href` added to `<main>` as `#e2e-link`, boosted as the
    /// page's own links are: htmx processes the region again.
    pub async fn add_link(&self, href: &str) -> WebDriverResult<()> {
        self.eval(&format!(
            "const a = document.createElement('a');
             a.href = {href:?}; a.id = 'e2e-link'; a.textContent = 'e2e';
             document.querySelector('main').append(a);
             htmx.process(document.getElementById('page'))"
        ))
        .await
        .map(drop)
    }

    /// Mark the document, so a later [`Page::in_place`] tells a move in
    /// place (the mark kept) from a full load (a new document, no mark).
    pub async fn mark(&self) -> WebDriverResult<()> {
        self.eval("window.__e2eMark = true").await.map(drop)
    }

    /// Whether the document marked by [`Page::mark`] is still the one shown.
    pub async fn in_place(&self) -> WebDriverResult<bool> {
        Ok(self.eval("return window.__e2eMark === true").await? == Value::Bool(true))
    }
}
