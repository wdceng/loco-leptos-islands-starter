//! Freeing an island once it has left the page. Leptos never does: the
//! code its `#[island]` macro generates runs the island under an owner of
//! its own, hydrates it, and then throws the result away, view state and
//! owner together (`std::mem::forget`, with a `TODO better cleanup` beside
//! it in leptos_macro). Whole page loads hide that: they throw the page
//! away with all its islands. An island that htmx swaps out of a page
//! stays in the tab's memory: its signals, effects, listeners and view
//! state, until the next page load.
//!
//! Fixed from our side, without touching Leptos: every island's body runs
//! through [`collected`], which returns the island's view wrapped in
//! [`Collected`] along with the island's owner. When the browser hydrates
//! the island, [`Collected`] keeps the real view state and the owner in a
//! registry and hands Leptos an empty shell ([`CollectedState`]) to forget.
//! A sweep then looks at the registry every 30 s (2 s in a debug build): an
//! island whose element has been off the page at two sweeps in a row is
//! freed, its view state first, then everything its owner holds (signals,
//! effects, `on_cleanup` hooks), the order tachys's own `OwnedView` keeps.
//! What stays is the owner's empty shell, a few dozen bytes, held by what
//! Leptos forgot. Two sweeps, not one: a morph swap or `hx-preserve` moves
//! an element within one task, which a timer never sees, but something
//! holding an element longer (a settle delay) must not lose its island.
//!
//! A stopgap until Leptos frees islands itself (leptos PR #4953, watched by
//! `tools/upstream.sh`); `docs/decisions.md` says when it can go.
//!
//! The view inside is type-erased (`AnyView`), for the debug build: there
//! cargo-leptos erases every component's type (`--cfg erase_components`),
//! which converts an island's output with `into_owned()` first. A wrapper
//! whose owned form is itself survives that; one around a generic view
//! would have to unwrap to it, and hydrate without ever reaching the
//! registry (the browser test below caught exactly that).
//!
//! On the server [`Collected`] renders as the view it wraps; the registry
//! and the sweep exist in the browser half only (`hydrate`).
//!
//! What an island must do itself: its window listeners are not part of its
//! view, so it keeps their handles and removes them in `on_cleanup`, which
//! runs when the island is freed. What belongs to the page rather than to
//! one island, set once for every island that comes later, stays in a
//! `thread_local!`. Event listeners in the view (`on:click`) need nothing:
//! tachys hands their closures to the browser, which frees them with the
//! element.
//!
//! Ported from the project this template shares its stack with, where it is
//! proven on the real page. The wrapper leans on tachys's view traits
//! (`Render`, `RenderHtml`, `Mountable`) and `AnyView`: a Leptos upgrade
//! that changes them stops the build here.

use leptos::{
    prelude::*,
    tachys::{
        html::attribute::{Attribute, any_attribute::AnyAttribute},
        hydration::Cursor,
        renderer::types::{Element, Node},
        ssr::StreamBuilder,
        view::{
            Mountable, Position, PositionState, Render, RenderHtml,
            add_attr::AddAnyAttr,
            any_view::{AnyView, AnyViewState, IntoAny},
        },
    },
};

/// An island's body, so the browser can free the island once it has left
/// the page. Every `#[island]` returns one: its whole body goes inside
/// (`collected(move || { … })`), the signals it creates, its listeners'
/// handles and its view (a test in islands.rs checks every island does).
/// The owner is the island's own, which the macro gives it; outside an
/// island, a new one.
pub fn collected<V: IntoView>(body: impl FnOnce() -> V) -> Collected {
    let owner = Owner::current().unwrap_or_default();
    let view = owner.with(|| body().into_any());
    Collected { view, owner }
}

/// What [`collected`] returns: the island's view, type-erased, and the
/// owner the sweep cleans up once the island has left the page.
pub struct Collected {
    view: AnyView,
    owner: Owner,
}

/// [`Collected`]'s view state: the real one where the view was built in
/// the browser without hydrating (never for an island, which always
/// hydrates), empty after hydration, the real one being in the registry.
pub struct CollectedState(Option<AnyViewState>);

impl Render for Collected {
    type State = CollectedState;

    fn build(self) -> Self::State {
        CollectedState(Some(self.view.build()))
    }

    fn rebuild(self, state: &mut Self::State) {
        if let Some(state) = &mut state.0 {
            self.view.rebuild(state);
        }
    }
}

impl AddAnyAttr for Collected {
    // Attributes are never spread onto an island; if one were, it would
    // stay collected: the result is erased again.
    type Output<SomeNewAttr: Attribute> = Collected;

    fn add_any_attr<NewAttr: Attribute>(self, attr: NewAttr) -> Self::Output<NewAttr>
    where
        Self::Output<NewAttr>: RenderHtml,
    {
        Collected {
            view: self.view.add_any_attr(attr).into_any(),
            owner: self.owner,
        }
    }
}

impl RenderHtml for Collected {
    type AsyncOutput = Collected;
    type Owned = Collected;

    const MIN_LENGTH: usize = <AnyView as RenderHtml>::MIN_LENGTH;

    fn dry_resolve(&mut self) {
        self.view.dry_resolve();
    }

    async fn resolve(self) -> Self::AsyncOutput {
        Collected {
            view: self.view.resolve().await,
            owner: self.owner,
        }
    }

    fn html_len(&self) -> usize {
        self.view.html_len()
    }

    fn to_html_with_buf(
        self,
        buf: &mut String,
        position: &mut Position,
        escape: bool,
        mark_branches: bool,
        extra_attrs: Vec<AnyAttribute>,
    ) {
        self.view
            .to_html_with_buf(buf, position, escape, mark_branches, extra_attrs);
    }

    fn to_html_async_with_buf<const OUT_OF_ORDER: bool>(
        self,
        buf: &mut StreamBuilder,
        position: &mut Position,
        escape: bool,
        mark_branches: bool,
        extra_attrs: Vec<AnyAttribute>,
    ) where
        Self: Sized,
    {
        self.view.to_html_async_with_buf::<OUT_OF_ORDER>(
            buf,
            position,
            escape,
            mark_branches,
            extra_attrs,
        );
    }

    /// Hydrates the view, then keeps its state and the island's owner in
    /// the registry and returns an empty state for Leptos to forget. The
    /// node the cursor is on when hydration starts (the `<leptos-island>`
    /// element, or its first child) leaves the page with the island: it is
    /// what the sweep watches.
    fn hydrate<const FROM_SERVER: bool>(
        self,
        cursor: &Cursor,
        position: &PositionState,
    ) -> Self::State {
        let island = cursor.current();
        let state = self.view.hydrate::<FROM_SERVER>(cursor, position);
        #[cfg(feature = "hydrate")]
        {
            sweep::register(island, self.owner, state);
            CollectedState(None)
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = (island, self.owner);
            CollectedState(Some(state))
        }
    }

    fn into_owned(self) -> Self::Owned {
        self
    }
}

impl Mountable for CollectedState {
    fn unmount(&mut self) {
        if let Some(state) = &mut self.0 {
            state.unmount();
        }
    }

    fn mount(&mut self, parent: &Element, marker: Option<&Node>) {
        if let Some(state) = &mut self.0 {
            state.mount(parent, marker);
        }
    }

    fn insert_before_this(&self, child: &mut dyn Mountable) -> bool {
        self.0
            .as_ref()
            .is_some_and(|state| state.insert_before_this(child))
    }

    fn elements(&self) -> Vec<Element> {
        self.0.as_ref().map(Mountable::elements).unwrap_or_default()
    }
}

/// The registry of hydrated islands and the sweep that frees the ones gone
/// from the page. Browser half only.
#[cfg(feature = "hydrate")]
mod sweep {
    use std::{
        cell::{Cell, RefCell},
        time::Duration,
    };

    use leptos::{
        prelude::{Owner, set_interval},
        tachys::view::any_view::AnyViewState,
    };
    use web_sys::Node;

    /// How often the sweep runs: rarely in production, where a few freed
    /// islands waiting half a minute cost nothing; often in a debug build,
    /// so a check in the browser need not wait.
    const EVERY: Duration = if cfg!(debug_assertions) {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(30)
    };

    /// Sweeps in a row an island's element must be off the page before the
    /// island is freed.
    const STRIKES: u8 = 2;

    /// One hydrated island: the node it is watched by, its owner and its
    /// view state.
    struct Entry {
        island: Node,
        owner: Owner,
        state: AnyViewState,
        missing: u8,
    }

    thread_local! {
        static LIVE: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
        static STARTED: Cell<bool> = const { Cell::new(false) };
    }

    /// Keeps a hydrated island; the first one starts the sweep, for the
    /// page's life.
    pub(super) fn register(island: Node, owner: Owner, state: AnyViewState) {
        #[cfg(debug_assertions)]
        mark_awake(&island);
        LIVE.with_borrow_mut(|live| {
            live.push(Entry {
                island,
                owner,
                state,
                missing: 0,
            });
        });
        // Not in the browser tests (below), which sweep by hand: a timer's
        // sweep landing between a test's steps would make it flaky.
        if !cfg!(test) && !STARTED.replace(true) {
            set_interval(sweep, EVERY);
        }
    }

    /// A debug build's mark on an island's `<leptos-island>` once it is
    /// awake, `data-awake`, which a browser check can wait for: Leptos's
    /// loader says nothing when its islands have woken, its
    /// `__hydrateIsland` set before the WASM even starts. The node
    /// hydration began at is the element or its first child. Never in a
    /// release build.
    #[cfg(debug_assertions)]
    fn mark_awake(island: &Node) {
        use leptos::wasm_bindgen::JsCast;
        let element = island
            .dyn_ref::<web_sys::Element>()
            .cloned()
            .or_else(|| island.parent_element());
        if let Some(found) =
            element.and_then(|element| element.closest("leptos-island").ok().flatten())
        {
            let _ = found.set_attribute("data-awake", "");
        }
    }

    /// Counts a strike for every island off the page, clears it for every
    /// island on it, and frees the islands at [`STRIKES`]. They are taken
    /// out of the registry first and freed after: freeing runs the
    /// islands' cleanups, which must find the registry free to use.
    fn sweep() {
        let gone: Vec<Entry> = LIVE.with_borrow_mut(|live| {
            for entry in live.iter_mut() {
                if entry.island.is_connected() {
                    entry.missing = 0;
                } else {
                    entry.missing += 1;
                }
            }
            let (gone, kept) = std::mem::take(live)
                .into_iter()
                .partition(|entry| entry.missing >= STRIKES);
            *live = kept;
            gone
        });
        let freed = gone.len();
        for Entry {
            island,
            owner,
            state,
            ..
        } in gone
        {
            // The view first (its effects, its element handles), then
            // whatever the owner holds (signals, effects, `on_cleanup`),
            // then the node, which the browser can then free too.
            drop(state);
            owner.cleanup();
            drop(owner);
            drop(island);
        }
        if cfg!(debug_assertions) && freed > 0 {
            let live = LIVE.with_borrow(Vec::len);
            leptos::logging::log!("islands: {live} live, {freed} freed");
        }
        #[cfg(debug_assertions)]
        report_live();
    }

    /// A debug build's count of the islands the registry holds, on the
    /// page's root (`data-islands-live`) after every sweep, which a memory
    /// check compares with the islands on the page: islands kept after
    /// they left are what the collector is for. Never in a release build.
    #[cfg(debug_assertions)]
    fn report_live() {
        if let Some(root) = leptos::prelude::document().document_element() {
            let live = LIVE.with_borrow(Vec::len);
            let _ = root.set_attribute("data-islands-live", &live.to_string());
        }
    }

    /// The registry and the sweep, in headless Chrome. Each test builds an
    /// island the way the `#[island]` macro does (an owner of its own,
    /// `OwnedView`, the view hydrated from its element and the result
    /// forgotten) around a body that goes through `collected`, then sweeps
    /// by hand. The tests share one page and so one registry: each empties
    /// it first and last. Built for wasm32 only, where `wasm-bindgen-test`
    /// is; run with a wasm-bindgen test runner and chromedriver
    /// (docs/testing.md, "The island collector").
    #[cfg(all(test, target_arch = "wasm32"))]
    mod tests {
        use std::{
            cell::Cell,
            rc::Rc,
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            },
        };

        use leptos::{
            prelude::*,
            tachys::{
                reactive_graph::OwnedView,
                view::{Position, RenderHtml, any_view::IntoAny},
            },
            wasm_bindgen::JsCast,
        };
        use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

        use super::{LIVE, sweep};
        use crate::collect::collected;

        wasm_bindgen_test_configure!(run_in_browser);

        /// An island built for a test: its element in the page, a signal
        /// its body created, and whether its `on_cleanup` has run.
        struct Island {
            element: web_sys::HtmlElement,
            signal: RwSignal<u8>,
            cleaned: Arc<AtomicBool>,
        }

        /// Hydrates an island in a new `<div>` of the page, holding what
        /// the server renders for its view (the empty view's marker), from
        /// the div's first child, as Leptos does inside a
        /// `<leptos-island>`. `erased` passes it through `into_any()`
        /// first, as a debug build does (`erase_components`).
        fn hydrated(erased: bool) -> Island {
            let element: web_sys::HtmlElement = document()
                .create_element("div")
                .expect("a div")
                .unchecked_into();
            element.set_inner_html(&().to_html());
            document()
                .body()
                .expect("a body")
                .append_child(&element)
                .expect("in the page");
            let cleaned = Arc::new(AtomicBool::new(false));
            let signal = Rc::new(Cell::new(None));
            let owner = Owner::new();
            let view = owner.with(|| {
                let (cleaned, signal) = (Arc::clone(&cleaned), Rc::clone(&signal));
                OwnedView::new(collected(move || {
                    signal.set(Some(RwSignal::new(1_u8)));
                    on_cleanup(move || cleaned.store(true, Ordering::SeqCst));
                }))
            });
            if erased {
                let state = view
                    .into_any()
                    .hydrate_from_position::<true>(&element, Position::FirstChild);
                std::mem::forget(state);
            } else {
                let state = view.hydrate_from_position::<true>(&element, Position::FirstChild);
                std::mem::forget(state);
            }
            Island {
                element,
                signal: signal.get().expect("the body ran"),
                cleaned,
            }
        }

        fn live() -> usize {
            LIVE.with_borrow(Vec::len)
        }

        /// Empties the registry, dropping what it held outside the borrow.
        fn drain() {
            let entries = LIVE.with_borrow_mut(std::mem::take);
            drop(entries);
        }

        fn cleaned(island: &Island) -> bool {
            island.cleaned.load(Ordering::SeqCst)
        }

        #[wasm_bindgen_test]
        fn freed_after_two_sweeps() {
            drain();
            let island = hydrated(false);
            assert_eq!(live(), 1, "registered when hydrated");
            island.element.remove();
            sweep();
            assert_eq!(live(), 1, "one sweep off the page is one strike");
            sweep();
            assert_eq!(live(), 0, "freed at the second strike");
            assert!(cleaned(&island), "its on_cleanup ran");
            assert_eq!(
                island.signal.try_get_untracked(),
                None,
                "its signal disposed"
            );
            drain();
        }

        #[wasm_bindgen_test]
        fn kept_while_on_the_page() {
            drain();
            let island = hydrated(false);
            sweep();
            sweep();
            assert_eq!(live(), 1);
            assert!(!cleaned(&island));
            assert_eq!(island.signal.try_get_untracked(), Some(1));
            island.element.remove();
            drain();
        }

        #[wasm_bindgen_test]
        fn one_strike_then_back_is_kept() {
            drain();
            let island = hydrated(false);
            island.element.remove();
            sweep();
            document()
                .body()
                .expect("a body")
                .append_child(&island.element)
                .expect("back in the page");
            sweep();
            sweep();
            assert_eq!(live(), 1, "back on the page clears the strike");
            assert!(!cleaned(&island));
            island.element.remove();
            drain();
        }

        #[wasm_bindgen_test]
        fn survives_type_erasure() {
            drain();
            let island = hydrated(true);
            assert_eq!(live(), 1, "registered through into_any()");
            island.element.remove();
            sweep();
            sweep();
            assert_eq!(live(), 0);
            assert!(cleaned(&island));
            drain();
        }
    }
}
