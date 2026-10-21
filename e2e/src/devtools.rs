//! Chrome's DevTools over its own socket (thirtyfour's `cdp-events`;
//! chromedriver passes commands on but no events): what the page logs, an
//! answer of htmx's changed on its way in, what the page holds in memory,
//! the visitor's settings emulated, and the accessibility audit.

use std::sync::{Arc, Mutex, PoisonError};

use futures_util::StreamExt;
use serde_json::{Value, json};
use thirtyfour::cdp::CdpSession;
use thirtyfour::prelude::*;
use tokio::task::AbortHandle;

use crate::Page;

/// axe-core, byte for byte from its npm package, with its MPL-2.0 licence
/// beside it (`e2e/vendor/`).
const AXE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/vendor/axe-core-4.14.0.min.js");

/// How htmx's answers reach the page from now on ([`Page::answers`]), as
/// something between the browser and the site would change them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Answer {
    /// As the site sent them.
    #[default]
    Plain,
    /// With another build's `X-Build`, as after a deploy.
    NewerBuild,
    /// With this status, as a gateway's error page.
    Status(u16),
}

/// What the page holds after a garbage collection ([`Page::held`]).
#[derive(Debug, Clone, Copy)]
pub struct Held {
    /// Documents: a page swapped out but kept is one more.
    pub documents: u64,
    /// Event listeners, all of the page's.
    pub listeners: u64,
    /// The JavaScript heap in use, in bytes.
    pub heap: f64,
    /// Detached element trees something still keeps, and their nodes:
    /// Chrome's count of all nodes includes those Blink hasn't freed yet.
    pub trees: usize,
    pub detached: usize,
    /// The WASM's memory, in bytes (the debug build's `__wasmMemory`,
    /// src/lib.rs). It only grows, in 64 KiB pages.
    pub wasm: u64,
}

/// What the watcher heard, shared with the page.
#[derive(Debug, Default)]
pub(crate) struct Heard {
    problems: Vec<String>,
    /// Failed loads a test causes on purpose: the status, and the full
    /// address Chrome's line ends with ([`Page::allow_failed_load`]).
    allowed: Vec<(u16, String)>,
    answer: Answer,
}

/// The shared state, locked briefly and never across an await.
pub(crate) type Shared = Arc<Mutex<Heard>>;

fn lock(heard: &Shared) -> std::sync::MutexGuard<'_, Heard> {
    heard.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Page {
    /// The page on `driver`, its DevTools session open and listened to:
    /// the page's errors, warnings and refused resources noted, and htmx's
    /// answers passed on as [`Page::answers`] says.
    pub(crate) async fn start(driver: WebDriver, base: String) -> WebDriverResult<Self> {
        let cdp = driver.cdp().connect().await?;
        let heard = Shared::default();
        // Subscribed before the domains are on, so nothing is missed.
        let watcher = watch(cdp.clone(), heard.clone());
        let page = Self {
            driver,
            cdp,
            base,
            heard,
            watcher,
        };
        // Once a session: `held` collects and reads with them.
        for domain in [
            "Runtime.enable",
            "Log.enable",
            "HeapProfiler.enable",
            "Performance.enable",
        ] {
            page.devtools(domain, json!({})).await?;
        }
        Ok(page)
    }

    /// The watcher stopped, with the session.
    pub(crate) fn stop(&self) {
        self.watcher.abort();
    }

    /// What the page logged that wasn't allowed.
    pub(crate) fn problems(&self) -> Vec<String> {
        let heard = lock(&self.heard);
        heard
            .problems
            .iter()
            .filter(|problem| {
                !heard.allowed.iter().any(|(status, url)| {
                    problem.contains(&format!("status of {status} "))
                        && problem.ends_with(&format!(" {url}"))
                })
            })
            .cloned()
            .collect()
    }

    /// A failed load the test causes on purpose, not counted: Chrome's
    /// "status of `status`" line for `path` on the run's site, that page
    /// alone, a 404 asked for or a 502 faked.
    pub fn allow_failed_load(&self, status: u16, path: &str) {
        let url = self.url(path);
        lock(&self.heard).allowed.push((status, url));
    }

    /// htmx's answers from now on, changed as `answer` says. Only requests
    /// htmx makes (`HX-Request`) change; every other passes as it came.
    /// Once on, the pausing stays on for the session, `Plain` passing each
    /// answer on unchanged: turning it off while the watcher passed a
    /// paused answer on left that request hanging in Chrome 114, a full
    /// load's islands never starting (2026-10-21).
    pub async fn answers(&self, answer: Answer) -> WebDriverResult<()> {
        lock(&self.heard).answer = answer;
        if answer == Answer::Plain {
            return Ok(());
        }
        // htmx's `fetch()` is resource type `Fetch` in Chrome 155 and `XHR`
        // in Chrome 114, the low bar (a probe, 2026-10-19).
        self.devtools(
            "Fetch.enable",
            json!({ "patterns": [
                { "urlPattern": "*", "resourceType": "Fetch", "requestStage": "Response" },
                { "urlPattern": "*", "resourceType": "XHR", "requestStage": "Response" },
            ] }),
        )
        .await
        .map(drop)
    }

    /// The page's size in CSS pixels, exactly (DevTools' device metrics).
    pub async fn viewport(&self, width: u32, height: u32) -> WebDriverResult<()> {
        self.devtools(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await
        .map(drop)
    }

    /// A media feature as the visitor's settings would set it, from now on
    /// (`prefers-reduced-motion`, `reduce`).
    pub async fn emulate(&self, feature: &str, value: &str) -> WebDriverResult<()> {
        self.devtools(
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": feature, "value": value }] }),
        )
        .await
        .map(drop)
    }

    /// The accessibility audit of the open page, axe-core's (vendored in
    /// `e2e/vendor/`, injected by the test, never served by the site): each
    /// rule broken, its impact and the first elements breaking it.
    pub async fn axe(&self) -> WebDriverResult<Vec<Value>> {
        let source = std::fs::read_to_string(AXE)
            .map_err(|e| WebDriverError::RequestFailed(format!("{AXE}: {e}")))?;
        self.driver.execute(&source, Vec::new()).await?;
        let found = self
            .driver
            .execute_async(
                "const done = arguments[arguments.length - 1];
                 axe.run(document, { resultTypes: ['violations'] })
                   .then(r => done(r.violations.map(v => ({ id: v.id, impact: v.impact,
                       help: v.help, nodes: v.nodes.slice(0, 3).map(n => n.target.join(' ') + ' | '
                           + n.html.slice(0, 120) + ' | ' + (n.any?.[0]?.message ?? '')) }))),
                         e => done([{ id: 'axe-error', help: String(e) }]))",
                Vec::new(),
            )
            .await?;
        Ok(found.json().as_array().cloned().unwrap_or_default())
    }

    /// Whether this Chrome's DevTools can list detached DOM nodes, which
    /// [`Page::held`] needs: Chrome 114's can't (the low bar). Asked of the
    /// browser, not by version.
    pub async fn can_count_detached(&self) -> WebDriverResult<bool> {
        self.dom_ready().await?;
        Ok(self
            .devtools("DOM.getDetachedDomNodes", json!({}))
            .await
            .is_ok())
    }

    /// The DOM domain on, with the current document requested: what
    /// `DOM.getDetachedDomNodes` needs before it answers.
    async fn dom_ready(&self) -> WebDriverResult<()> {
        self.devtools("DOM.enable", json!({})).await?;
        self.devtools("DOM.getDocument", json!({ "depth": 0 }))
            .await
            .map(drop)
    }

    /// What the page holds once the garbage is collected.
    pub async fn held(&self) -> WebDriverResult<Held> {
        self.devtools("HeapProfiler.collectGarbage", json!({}))
            .await?;
        let counters = self.devtools("Memory.getDOMCounters", json!({})).await?;
        let metrics = self.devtools("Performance.getMetrics", json!({})).await?;
        let heap = metrics["metrics"]
            .as_array()
            .and_then(|all| all.iter().find(|m| m["name"] == "JSHeapUsedSize"))
            .and_then(|m| m["value"].as_f64())
            .unwrap_or_default();
        self.dom_ready().await?;
        let found = self.devtools("DOM.getDetachedDomNodes", json!({})).await?;
        // Elements only (node type 1). With chromedriver attached, Chrome
        // keeps a swapped-out region's attribute nodes for a while: forty
        // after ten moves, none after twenty, and none at all without
        // chromedriver (a probe, 2026-10-18). A kept page is an element.
        let trees: Vec<Value> = found["detachedNodes"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter(|tree| tree["treeNode"]["nodeType"] == 1)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let detached = trees
            .iter()
            .map(|tree| tree["retainedNodeIds"].as_array().map_or(0, Vec::len))
            .sum();
        let wasm = self
            .eval("return window.__wasmMemory?.buffer.byteLength ?? 0")
            .await?;
        let count = |name: &str| counters[name].as_u64().unwrap_or_default();
        Ok(Held {
            documents: count("documents"),
            listeners: count("jsEventListeners"),
            heap,
            trees: trees.len(),
            detached,
            wasm: wasm.as_u64().unwrap_or_default(),
        })
    }

    /// A DevTools command on the page's session.
    pub async fn devtools(&self, method: &str, params: Value) -> WebDriverResult<Value> {
        self.cdp
            .send_raw(method, params)
            .await
            .map_err(|e| WebDriverError::RequestFailed(format!("{method}: {e}")))
    }
}

/// Listen to the session's events until stopped: problems noted in
/// `heard`, paused answers passed on.
fn watch(cdp: CdpSession, heard: Shared) -> AbortHandle {
    let mut events = cdp.subscribe_all();
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let params = &event.params;
            match event.method.as_str() {
                "Runtime.exceptionThrown" => {
                    let details = &params["exceptionDetails"];
                    let said = details["exception"]["description"]
                        .as_str()
                        .or_else(|| details["text"].as_str())
                        .unwrap_or("?");
                    note(&heard, format!("exception: {said}"));
                }
                "Runtime.consoleAPICalled"
                    if matches!(params["type"].as_str(), Some("error" | "warning")) =>
                {
                    let said: Vec<String> = params["args"]
                        .as_array()
                        .map(|args| {
                            args.iter()
                                .map(|a| match &a["value"] {
                                    Value::String(s) => s.clone(),
                                    Value::Null => {
                                        a["description"].as_str().unwrap_or("").to_owned()
                                    }
                                    other => other.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    note(
                        &heard,
                        format!(
                            "console.{}: {}",
                            params["type"].as_str().unwrap_or("?"),
                            said.join(" ")
                        ),
                    );
                }
                // CSP refusals and failed loads land here.
                "Log.entryAdded" => {
                    let entry = &params["entry"];
                    if matches!(entry["level"].as_str(), Some("error" | "warning")) {
                        note(
                            &heard,
                            format!(
                                "log.{}: {} {}",
                                entry["level"].as_str().unwrap_or("?"),
                                entry["text"].as_str().unwrap_or(""),
                                entry["url"].as_str().unwrap_or("")
                            ),
                        );
                    }
                }
                "Fetch.requestPaused" => pass_on(&cdp, &heard, params).await,
                _ => {}
            }
        }
    })
    .abort_handle()
}

fn note(heard: &Shared, problem: String) {
    lock(heard).problems.push(problem);
}

/// A paused answer passed on, changed when htmx asked for it and the test
/// said so ([`Page::answers`]). A request that failed before an answer
/// goes on unchanged. Best effort: an answer DevTools won't take is
/// noted, so the test fails saying why.
async fn pass_on(cdp: &CdpSession, heard: &Shared, params: &Value) {
    let id = params["requestId"].clone();
    if params.get("responseErrorReason").is_some() {
        let _ = cdp
            .send_raw("Fetch.continueRequest", json!({ "requestId": id }))
            .await;
        return;
    }
    let from_htmx = params["request"]["headers"]
        .as_object()
        .is_some_and(|headers| headers.keys().any(|k| k.eq_ignore_ascii_case("hx-request")));
    let answer = if from_htmx {
        lock(heard).answer
    } else {
        Answer::Plain
    };
    let mut code = params["responseStatusCode"].as_u64().unwrap_or(200);
    let mut headers = params["responseHeaders"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    match answer {
        Answer::Plain => {}
        Answer::NewerBuild => {
            for header in &mut headers {
                if header["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("x-build"))
                {
                    header["value"] = Value::from("ffffffffffffffff");
                }
            }
        }
        Answer::Status(status) => code = u64::from(status),
    }
    // DevTools wants the status whenever the headers are given.
    if let Err(e) = cdp
        .send_raw(
            "Fetch.continueResponse",
            json!({ "requestId": id, "responseCode": code, "responseHeaders": headers }),
        )
        .await
    {
        note(heard, format!("passing an answer on: {e}"));
    }
}
