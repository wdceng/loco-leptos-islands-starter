//! Moving between pages in place (docs/architecture.md, "Moves in place"):
//! a link or a form in the shell's `#page` brings the next page's region in
//! with no page load, its lazy island woken, its title, the focus on
//! `<main>` and the title read out; Back brings the page back where it was
//! scrolled; a missing page shows in the region; a newer build and a
//! gateway's error load the page plainly instead (`Navigation`,
//! src/islands.rs).

use app_e2e::{Answer, HOME_LINK, Page, RESET, WebDriverResult, browse};
use serde_json::Value;

/// The home page's title, from a full load.
async fn home_title(page: &Page) -> WebDriverResult<String> {
    page.open("/").await?;
    page.driver.title().await
}

/// From the reset page, the header's link home with htmx's answer changed
/// as `answer` says: the home page must load whole, not swap in.
async fn loads_plainly(page: &Page, answer: Answer) -> WebDriverResult<()> {
    page.open(RESET).await?;
    page.mark().await?;
    page.answers(answer).await?;
    page.press(HOME_LINK).await?;
    page.until(
        "the home page loaded whole",
        "return window.__e2eMark !== true && location.pathname === '/'
             && document.readyState === 'complete'",
    )
    .await?;
    page.answers(Answer::Plain).await?;
    page.ready().await
}

#[tokio::test]
async fn a_full_load_wakes_the_lazy_island() {
    browse(|page| async move {
        page.open("/").await?;
        assert!(page.says("Islands: running").await?, "the lazy island runs");
        let title = page.driver.title().await?;
        assert!(
            !title.contains("<!>"),
            "no hydration marker in the tab: {title}"
        );
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn a_link_moves_in_place_with_its_title_the_focus_and_the_announcement() {
    browse(|page| async move {
        let title = home_title(&page).await?;
        page.open(RESET).await?;
        page.mark().await?;
        page.move_to(HOME_LINK, "/").await?;
        assert!(page.in_place().await?, "no full load");
        assert!(
            page.says("Islands: running").await?,
            "the lazy island swapped in wakes"
        );
        assert_eq!(page.driver.title().await?, title, "the new page's title");
        assert_eq!(
            page.eval("return document.activeElement?.id").await?,
            Value::from("content"),
            "the focus on <main>"
        );
        // Read out a tick after the swap: htmx sets the title after it.
        page.until(
            "the title read out",
            &format!("return document.getElementById('page-announce').textContent === {title:?}"),
        )
        .await?;
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn back_comes_back_in_place_where_it_was_scrolled() {
    browse(|page| async move {
        // A short window, so the reset page scrolls.
        page.viewport(800, 300).await?;
        page.open(RESET).await?;
        page.mark().await?;
        page.eval("window.scrollTo(0, 120)").await?;
        let before = page.eval("return window.scrollY").await?;
        let before = before.as_f64().unwrap_or_default();
        assert!(before > 0.0, "the reset page scrolls in 300 pixels");
        page.move_to(HOME_LINK, "/").await?;
        page.eval("history.back()").await?;
        page.until(
            "the reset page back",
            &format!("return location.pathname + location.search === {RESET:?}"),
        )
        .await?;
        page.settled().await?;
        assert!(page.in_place().await?, "Back in place");
        page.until(
            "the scroll put back",
            &format!("return Math.abs(window.scrollY - {before}) < 2"),
        )
        .await?;
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn a_missing_page_shows_in_the_region() {
    browse(|page| async move {
        // The 404 asked for, which Chrome logs.
        page.allow_failed_load(404, "/no-such-page");
        page.open("/").await?;
        page.mark().await?;
        page.add_link("/no-such-page").await?;
        page.press("#e2e-link").await?;
        page.until(
            "the not-found page in the region",
            "return document.body.innerText.includes('Page not found')",
        )
        .await?;
        assert!(page.in_place().await?, "no full load");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn a_newer_build_loads_the_next_page_plainly() {
    browse(|page| async move { loads_plainly(&page, Answer::NewerBuild).await }).await;
}

#[tokio::test]
async fn a_refused_form_swaps_in_and_keeps_the_address() {
    browse(|page| async move {
        // The refusal's 400, which Chrome logs.
        page.allow_failed_load(400, "/reset");
        page.open(RESET).await?;
        page.mark().await?;
        page.eval(
            "document.getElementById('password').value = 'short';
             document.querySelector('main form button').click()",
        )
        .await?;
        page.until(
            "the refused form in the region",
            "return document.body.innerText.includes('must be 8 to 128')",
        )
        .await?;
        assert!(page.in_place().await?, "no full load");
        assert_eq!(page.path().await?, RESET, "the address stays the page's");
        Ok(())
    })
    .await;
}

#[tokio::test]
async fn a_gateway_error_loads_the_link_plainly() {
    browse(|page| async move {
        // The faked 502, which Chrome logs.
        page.allow_failed_load(502, "/");
        loads_plainly(&page, Answer::Status(502)).await
    })
    .await;
}
