//! Moving between pages in place (docs/architecture.md, "Moves in place"):
//! a link or a form in the shell's `#page` brings the next page's region in
//! with no page load, its lazy island woken, its title, the focus on
//! `<main>` and the title read out; Back brings the page back where it was
//! scrolled; a missing page shows in the region; a newer build and a
//! gateway's error load the page plainly instead (`Navigation`,
//! src/islands.rs).

use app_e2e::{Answer, HOME_LINK, Page, RESET, WebDriverResult, browse};
use serde_json::{Value, json};

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

/// htmx gives the focus back to an element with the same `id` in the new
/// page; `Navigation` blurs before a move's swap, so no field takes the
/// focus on the way to `<main>`, where a field styled on its focus would
/// open and close over the new page. The link keeps the field's focus when
/// pressed, as a search's suggestion does; a plain link takes the focus
/// itself.
#[tokio::test]
async fn a_move_focuses_no_field_on_its_way() {
    browse(|page| async move {
        page.open(RESET).await?;
        page.mark().await?;
        let again = format!("{RESET}&again=1");
        page.add_link(&again).await?;
        page.eval(
            "window.__focused = [];
             addEventListener('focusin', e => __focused.push(e.target.id || e.target.tagName), true);
             document.getElementById('e2e-link').addEventListener('mousedown', e => e.preventDefault());
             document.getElementById('password').focus()",
        )
        .await?;
        page.move_to("#e2e-link", &again).await?;
        assert!(page.in_place().await?, "no full load");
        assert_eq!(
            page.eval("return __focused").await?,
            json!(["password", "content"]),
            "the field focused by the test, then <main> alone"
        );
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
        // A link home at the end of `<main>`, scrolled to, as a person
        // scrolls to what they press (the header's link is above).
        page.add_link("/").await?;
        page.eval("document.getElementById('e2e-link').scrollIntoView({ block: 'end' })")
            .await?;
        let before = page.eval("return window.scrollY").await?;
        let before = before.as_f64().unwrap_or_default();
        assert!(before > 0.0, "the reset page scrolls in 300 pixels");
        page.move_to("#e2e-link", "/").await?;
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
        // As Enter in the field sends it: the focus in the field.
        page.eval(
            "const field = document.getElementById('password');
             field.value = 'short'; field.focus();
             field.form.requestSubmit()",
        )
        .await?;
        page.until(
            "the refused form in the region",
            "return document.body.innerText.includes('must be 8 to 128')",
        )
        .await?;
        assert!(page.in_place().await?, "no full load");
        assert_eq!(page.path().await?, RESET, "the address stays the page's");
        page.settled().await?;
        assert_eq!(
            page.eval("return document.activeElement?.id").await?,
            Value::from("password"),
            "the focus in the field for the next try, not on <main>"
        );
        Ok(())
    })
    .await;
}

/// A second press while the form is on its way sends nothing: htmx 4's
/// default queues one more request (`queue first`), which would post the
/// reset again with a token the first post used up, and swap "expired"
/// over "changed". The form drops it (`hx-sync`).
#[tokio::test]
async fn a_second_press_sends_no_second_post() {
    browse(|page| async move {
        // The e2e token is no real one: the post is refused as expired.
        page.allow_failed_load(400, "/reset");
        page.open(RESET).await?;
        page.mark().await?;
        // Every POST htmx sends, counted where it leaves: htmx binds
        // `window.fetch` per request, a queued one too.
        page.eval(
            "window.__posts = 0;
             const fetchFirst = window.fetch;
             window.fetch = (...args) => {
                 const method = (args[1]?.method ?? args[0]?.method ?? 'GET').toUpperCase();
                 if (method === 'POST') window.__posts++;
                 return fetchFirst(...args);
             };
             const field = document.getElementById('password');
             field.value = 'long enough to pass';
             field.form.requestSubmit();
             field.form.requestSubmit()",
        )
        .await?;
        page.until(
            "the answer in the region",
            "return !document.querySelector('main form')",
        )
        .await?;
        page.settled().await?;
        // Long enough for a queued second post to have gone out.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert_eq!(
            page.eval("return window.__posts").await?,
            Value::from(1),
            "one press, one post"
        );
        Ok(())
    })
    .await;
}

/// While the form is on its way, slower than 200 ms, it's `aria-busy` and
/// its button takes the `busy:` look; the answer's page holds neither.
#[tokio::test]
async fn a_sent_form_looks_busy_until_its_answer() {
    browse(|page| async move {
        // The e2e token is no real one: the post is refused as expired.
        page.allow_failed_load(400, "/reset");
        page.open(RESET).await?;
        let button = "document.querySelector('main form [type=submit]')";
        let idle = page
            .text(&format!("return getComputedStyle({button}).opacity"))
            .await?;
        // A slow network: the answer takes longer than the look's delay.
        page.devtools("Network.enable", json!({})).await?;
        page.devtools(
            "Network.emulateNetworkConditions",
            json!({ "offline": false, "latency": 1500, "downloadThroughput": -1, "uploadThroughput": -1 }),
        )
        .await?;
        page.eval(
            "const field = document.getElementById('password');
             field.value = 'long enough to pass';
             field.form.requestSubmit()",
        )
        .await?;
        page.until(
            "the form busy and its button's look changed",
            &format!(
                "return document.querySelector('main form')?.getAttribute('aria-busy') === 'true'
                     && getComputedStyle({button}).opacity !== {idle:?}"
            ),
        )
        .await?;
        page.until(
            "the answer in the region",
            "return !document.querySelector('main form')",
        )
        .await?;
        page.settled().await?;
        assert_eq!(
            page.eval("return !!document.querySelector('[aria-busy=true], .htmx-request')")
                .await?,
            Value::Bool(false),
            "nothing busy left after the answer"
        );
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
