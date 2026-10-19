//! Leaks. Twenty moves in place between the home page and the reset page,
//! and twenty steps Back and Forward, each leave the page holding about
//! what it held before: no swapped-out page's documents, nodes or
//! listeners kept, the JavaScript heap and the WASM's memory flat, and the
//! collector holding no more islands than the page shows (src/collect.rs;
//! the debug build's `data-islands-live`). The home page's lazy
//! `IslandsCheck` is loaded, woken and freed on every round.
//!
//! The sister app's method: measured three times, the garbage collected
//! first, before the rounds, halfway and at the end. The end against the
//! start allows some growth, a cache filling up; the second half against
//! the first allows almost none, as a leak grows with every round while a
//! cache levels off (the slope, which tools like fuite and memlab look
//! for, rather than one total with a margin a slow leak hides in). The DOM
//! is read as Chrome's detached trees, the nodes off the page something
//! still keeps, never as its count of all nodes, which includes the ones
//! Blink hasn't freed yet.

use std::time::Duration;

use app_e2e::{Held, Page, RESET, WebDriverResult, browse};
use serde_json::json;

/// How many times each round is repeated.
const ROUNDS: usize = 20;

/// Long enough for the collector's two sweeps (2 s apart in a debug build)
/// to free the islands that left.
const SWEEPS: Duration = Duration::from_secs(5);

/// How much the heap may grow over the second half, in bytes: the
/// measure's own noise after a collection, not a page's worth.
const HEAP_SLOPE: f64 = 1_000_000.0;

/// How many detached nodes may be kept beyond the start: about one page
/// region, the last one htmx swapped out, which it keeps.
const ONE_REGION: usize = 1_000;

/// How much the WASM's memory may grow over the second half: one 64 KiB
/// page, the allocator's own step.
const WASM_SLOPE: u64 = 64 * 1024;

/// What the page holds once the collector has swept twice.
async fn settled_memory(page: &Page) -> WebDriverResult<Held> {
    tokio::time::sleep(SWEEPS).await;
    page.held().await
}

/// One move away from home to the reset page by a link added for it, and
/// back by the header's link.
async fn round_trip(page: &Page) -> WebDriverResult<()> {
    page.add_link(RESET).await?;
    page.move_to("#e2e-link", RESET).await?;
    page.move_to("header a[href='/']", "/").await
}

/// `round` twice to warm up, then measured before, halfway and after
/// [`ROUNDS`] more, and judged by [`about_the_same`].
///
/// DevTools' log is off while it measures: it keeps the element each entry
/// names, and Chrome logs a notice about the reset form ("Password forms
/// should have username fields") on every visit, so with it on each Back
/// kept one old `<main>` (a probe, 2026-10-18). A browser without DevTools
/// keeps none. Off in both sessions, the test's and chromedriver's own,
/// which has it on for its browser logs. The move tests keep the log for
/// CSP and load errors.
async fn measure<F, Fut>(page: &Page, what: &str, round: F) -> WebDriverResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = WebDriverResult<()>>,
{
    if !page.can_count_detached().await? {
        eprintln!("{what}: skipped, this Chrome's DevTools can't list detached nodes");
        return Ok(());
    }
    page.devtools("Log.disable", json!({})).await?;
    page.driver.cdp().send_raw("Log.disable", json!({})).await?;
    for _ in 0..2 {
        round().await?;
    }
    let before = settled_memory(page).await?;
    for _ in 0..ROUNDS / 2 {
        round().await?;
    }
    let half = settled_memory(page).await?;
    for _ in 0..ROUNDS / 2 {
        round().await?;
    }
    assert!(page.in_place().await?, "{what}: every move in place");
    let after = settled_memory(page).await?;
    eprintln!("{what}: before {before:?}\n  halfway {half:?}\n  after {after:?}");
    about_the_same(page, what, before, half, after).await
}

/// The page holding about what it held: against `before` the rounds, some
/// growth allowed (a leak of a page's worth a round would be twenty pages'
/// worth by now); against `half` of them, almost none, the slope.
async fn about_the_same(
    page: &Page,
    what: &str,
    before: Held,
    half: Held,
    after: Held,
) -> WebDriverResult<()> {
    assert!(
        after.documents <= before.documents,
        "{what}: documents kept, {} then {}",
        before.documents,
        after.documents
    );
    assert!(
        after.detached <= before.detached + ONE_REGION,
        "{what}: detached nodes kept, {} then {} in {} trees",
        before.detached,
        after.detached,
        after.trees
    );
    assert!(
        after.listeners <= before.listeners + before.listeners / 5 + 20,
        "{what}: listeners grew, {} then {}",
        before.listeners,
        after.listeners
    );
    assert!(
        after.heap <= before.heap * 1.5,
        "{what}: the heap grew, {} then {}",
        before.heap,
        after.heap
    );
    // The second half adding next to nothing to the first.
    assert!(
        after.trees <= half.trees + 1 && after.detached <= half.detached + 200,
        "{what}: detached trees still growing, {} nodes in {} halfway then {} in {}",
        half.detached,
        half.trees,
        after.detached,
        after.trees
    );
    assert!(
        after.listeners <= half.listeners + 10,
        "{what}: listeners still growing, {} halfway then {}",
        half.listeners,
        after.listeners
    );
    assert!(
        after.heap <= half.heap + HEAP_SLOPE,
        "{what}: the heap still growing, {} halfway then {}",
        half.heap,
        after.heap
    );
    assert!(
        after.wasm <= half.wasm + WASM_SLOPE,
        "{what}: the WASM's memory still growing, {} halfway then {}",
        half.wasm,
        after.wasm
    );
    let islands = page
        .eval(
            "return [Number(document.documentElement.dataset.islandsLive ?? 0),
                     document.querySelectorAll('leptos-island').length]",
        )
        .await?;
    let (live, shown) = (
        islands[0].as_u64().unwrap_or_default(),
        islands[1].as_u64().unwrap_or_default(),
    );
    assert!(
        live <= shown,
        "{what}: the collector holds {live} islands, the page shows {shown}"
    );
    Ok(())
}

#[tokio::test]
async fn an_island_that_leaves_is_freed() {
    browse(|page| async move {
        page.open("/").await?;
        page.mark().await?;
        page.add_link(RESET).await?;
        page.move_to("#e2e-link", RESET).await?;
        assert!(page.in_place().await?, "no full load");
        // `IslandsCheck` swapped out and freed after two sweeps;
        // `Navigation`, outside the region, kept.
        page.until_within(
            "one island live",
            "return document.documentElement.dataset.islandsLive === '1'",
            Duration::from_secs(10),
        )
        .await
    })
    .await;
}

#[tokio::test]
async fn moves_leave_nothing_behind() {
    browse(|page| async move {
        page.open("/").await?;
        page.mark().await?;
        measure(&page, "moves", || round_trip(&page)).await
    })
    .await;
}

#[tokio::test]
async fn back_and_forward_leave_nothing_behind() {
    browse(|page| async move {
        page.open("/").await?;
        page.mark().await?;
        page.add_link(RESET).await?;
        page.move_to("#e2e-link", RESET).await?;
        let step = || async {
            page.eval("history.back()").await?;
            page.until("Back home", "return location.pathname === '/'")
                .await?;
            page.settled().await?;
            page.eval("history.forward()").await?;
            page.until(
                "Forward to the reset page",
                "return location.pathname === '/reset'",
            )
            .await?;
            page.settled().await
        };
        measure(&page, "Back and Forward", step).await
    })
    .await;
}
