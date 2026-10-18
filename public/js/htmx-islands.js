// For htmx 4 only (htmx 2 names its events differently: htmx:afterSwap).
// Keeps Leptos islands alive in HTML htmx swaps in. The one hand-written
// script in the template: plumbing, no page logic (CLAUDE.md, "Islands").

// In blocks, so these consts stay out of the page's global scope.
{
  // 1. Leptos's island loader waits for idle time with no timeout, and
  //    Chrome 114 to 123 can stop giving any after many whole loads in one
  //    tab: then no island starts. Every idle callback gets a 2 s timeout,
  //    as Leptos's own hydration script has. A `defer` script before
  //    <HydrationScripts/> runs before its loader, a module script.
  //    Remove once a Leptos release passes its own (docs/decisions.md).
  const idle = window.requestIdleCallback;
  if (idle) {
    window.requestIdleCallback = (callback, options) =>
      idle.call(window, callback, { timeout: 2000, ...options });
  }

  // 2. Chrome 150 to 156 drops the comments Leptos hydrates by when
  //    parseHTMLUnsafe gets one argument; with options it keeps them
  //    (https://github.com/bigskysoftware/htmx/issues/4147, the htmx
  //    maintainer's way). Remove once Chrome 157 is everywhere.
  const parse = Document.parseHTMLUnsafe?.bind(Document);
  if (parse) {
    Document.parseHTMLUnsafe = (html, options) => parse(html, { ...options });
  }
}

{
  // 3. Wake islands htmx swaps in, as Leptos's own router does.
  //    Islands already on the page are Leptos's first pass: mark them.
  for (const el of document.querySelectorAll("leptos-island")) {
    el.$$hydrated = true;
  }
  const wake = () => {
    const hydrate = window.__hydrateIsland;
    for (const el of document.querySelectorAll("leptos-island")) {
      if (el.$$hydrated) continue;
      el.$$hydrated = true;
      // A failure while the wasm still starts: the first pass takes it.
      if (hydrate) hydrate(el, el.dataset.component).catch(() => {});
    }
  };
  document.addEventListener("htmx:after:swap", wake);

  // 4. htmx 4 doesn't watch the page: process hx-* added later, and wake
  //    islands added later (the swap event may fire on a removed element).
  const HX =
    "[hx-get],[hx-post],[hx-put],[hx-patch],[hx-delete],[hx-action],[hx-trigger]";
  new MutationObserver((records) => {
    let islands = false;
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        // Not where htmx was blocked: the page works as plain links then.
        if (window.htmx && (node.matches(HX) || node.querySelector(HX)))
          window.htmx.process(node);
        islands ||=
          node.matches("leptos-island") ||
          !!node.querySelector("leptos-island");
      }
    }
    if (islands) wake();
  }).observe(document.documentElement, { childList: true, subtree: true });
}
